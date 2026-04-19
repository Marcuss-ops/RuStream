//! Audio resampling utilities using FFmpeg.
//!
//! This module provides functions to resample audio files to a target sample rate
//! and channel count, outputting to WAV (PCM s16le) or M4A/MP4 (AAC) formats.
//!
//! # Features
//! - Resample audio to arbitrary sample rates and channel configurations
//! - Output to WAV (PCM s16le) or AAC (M4A/MP4)
//! - Automatic channel layout detection and correction
//! - Frame-aligned encoding for AAC
//! - O(n) sample buffer management (read offset + periodic compaction)

#![allow(unsafe_code)] // Required for FFmpeg raw pointer operations on audio sample buffers

use ffmpeg_next as ff;
use ff::util::error::EAGAIN;
use crate::core::{MediaError, MediaErrorCode, MediaResult};

/// Periodic compaction threshold: compact when read_pos exceeds this many samples.
/// This turns O(n²) drain-into-shift into amortized O(n).
const COMPACTION_THRESHOLD: usize = 32768;

/// Sample buffer with read-offset tracking to avoid O(n²) drain operations.
///
/// Instead of `drain(0..frame_size)` on every AAC frame (which shifts all
/// remaining elements and causes O(n²) total work), we track a read position
/// and compact periodically.
struct SampleBuffer {
    channels: Vec<Vec<f32>>,
    read_pos: usize,
}

impl SampleBuffer {
    #[allow(dead_code)]
    fn new(channels: usize, capacity: usize) -> Self {
        Self {
            channels: vec![Vec::with_capacity(capacity); channels],
            read_pos: 0,
        }
    }

    /// Number of available samples (beyond the read position)
    #[allow(dead_code)]
    #[inline]
    fn available(&self) -> usize {
        self.channels[0].len().saturating_sub(self.read_pos)
    }

    /// Consume `count` samples by advancing the read position.
    /// Compacts the buffer when read_pos exceeds the threshold.
    #[inline]
    fn consume(&mut self, count: usize) {
        self.read_pos += count;
        // Compact periodically: O(n) amortized instead of O(n) per frame
        if self.read_pos >= COMPACTION_THRESHOLD {
            for ch in &mut self.channels {
                ch.drain(..self.read_pos);
            }
            self.read_pos = 0;
        }
    }

    /// Read `count` samples from channel `ch` as a slice (zero-copy view).
    #[allow(dead_code)]
    #[inline]
    fn read(&self, ch: usize, count: usize) -> &[f32] {
        let start = self.read_pos;
        &self.channels[ch][start..start + count]
    }

    /// Extend channel `ch` with samples from a slice.
    #[inline]
    fn extend(&mut self, ch: usize, samples: &[f32]) {
        self.channels[ch].extend_from_slice(samples);
    }

    /// Resize channel `ch` to `new_len` (logical, excluding consumed prefix).
    #[inline]
    fn resize(&mut self, ch: usize, new_len: usize) {
        let actual_len = new_len + self.read_pos;
        if self.channels[ch].len() < actual_len {
            self.channels[ch].resize(actual_len, 0.0);
        } else {
            self.channels[ch].truncate(actual_len);
        }
    }

    /// Get the logical length of channel `ch` (excluding consumed prefix).
    #[inline]
    fn len(&self, ch: usize) -> usize {
        self.channels[ch].len().saturating_sub(self.read_pos)
    }

    /// Get pointer to the current read position for channel `ch`.
    #[inline]
    fn as_ptr(&self, ch: usize) -> *const f32 {
        self.channels[ch].as_ptr().wrapping_add(self.read_pos)
    }

    /// Compact any remaining consumed prefix (call before final encode).
    fn compact(&mut self) {
        if self.read_pos > 0 {
            for ch in &mut self.channels {
                ch.drain(..self.read_pos);
            }
            self.read_pos = 0;
        }
    }
}

/// Resample an audio file to target sample rate and channel count.
/// Output format determined by extension: .wav = PCM s16le, .m4a/.mp4 = AAC
///
/// # Arguments
/// * `input_path` - Path to input audio file
/// * `output_path` - Path to output file (.wav, .m4a, or .mp4)
/// * `target_rate` - Target sample rate (default: 48000 Hz)
/// * `target_ch` - Target channels (default: 2 for stereo)
///
/// # Returns
/// * `Ok(true)` on success
/// * `Err(MediaError)` on failure
pub fn resample_audio_file(
    input_path: &str,
    output_path: &str,
    target_rate: u32,
    target_ch: u32,
) -> MediaResult<bool> {
    let _ = ff::init(); // Ignore error if already initialized

    // Determine output format
    let output_lower = output_path.to_lowercase();
    let is_wav = output_lower.ends_with(".wav");
    let is_m4a = output_lower.ends_with(".m4a") || output_lower.ends_with(".mp4");

    // Open input
    let mut in_ctx = ff::format::input(input_path)
        .map_err(|e| MediaError::new(MediaErrorCode::DecodeFailed, format!("Cannot open '{}': {}", input_path, e)))?;

    let audio_stream = in_ctx.streams()
        .find(|s| s.parameters().medium() == ff::media::Type::Audio)
        .ok_or_else(|| MediaError::new(MediaErrorCode::DecodeFailed, "No audio stream found"))?;
    let audio_idx = audio_stream.index();

    // Setup decoder
    let mut decoder = ff::codec::context::Context::from_parameters(audio_stream.parameters())
        .map_err(|e| MediaError::new(MediaErrorCode::DecodeFailed, format!("Decoder init: {}", e)))?
        .decoder().audio()
        .map_err(|e| MediaError::new(MediaErrorCode::DecodeFailed, format!("Audio decoder: {}", e)))?;

    let src_rate = decoder.rate();
    let mut src_layout = decoder.channel_layout();
    let src_fmt = decoder.format();

    // Fix empty channel layout
    if src_layout.is_empty() {
        src_layout = match decoder.channels() {
            1 => ff::channel_layout::ChannelLayout::MONO,
            2 => ff::channel_layout::ChannelLayout::STEREO,
            _ => ff::channel_layout::ChannelLayout::STEREO,
        };
    }

    // Destination settings
    let dst_layout = if target_ch == 1 {
        ff::channel_layout::ChannelLayout::MONO
    } else {
        ff::channel_layout::ChannelLayout::STEREO
    };

    // Collect all packets
    let packets: Vec<ff::Packet> = in_ctx
        .packets()
        .filter_map(|(s, p)| if s.index() == audio_idx { Some(p) } else { None })
        .collect();

    // Create output
    let mut out_ctx = ff::format::output(output_path)
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Cannot create output: {}", e)))?;

    if is_wav {
        encode_wav(&packets, &mut decoder, src_fmt, src_layout, src_rate,
                   &mut out_ctx, target_rate, dst_layout)?;
    } else if is_m4a {
        encode_aac(&packets, &mut decoder, src_fmt, src_layout, src_rate,
                   &mut out_ctx, target_rate, dst_layout, target_ch as usize)?;
    } else {
        return Err(MediaError::new(MediaErrorCode::AudioResampleFailed, "Unsupported format. Use .wav, .m4a, or .mp4"));
    }

    out_ctx.write_trailer()
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Write trailer: {}", e)))?;

    Ok(true)
}

/// Encode to WAV (PCM s16le)
fn encode_wav(
    packets: &[ff::Packet],
    decoder: &mut ff::decoder::Audio,
    src_fmt: ff::util::format::Sample,
    src_layout: ff::channel_layout::ChannelLayout,
    src_rate: u32,
    out_ctx: &mut ff::format::context::Output,
    target_rate: u32,
    dst_layout: ff::channel_layout::ChannelLayout,
) -> MediaResult<()> {
    let codec = ff::encoder::find(ff::codec::Id::PCM_S16LE)
        .ok_or_else(|| MediaError::new(MediaErrorCode::AudioResampleFailed, "PCM_S16LE encoder not found"))?;

    let mut ost = out_ctx.add_stream(codec)
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Add stream: {}", e)))?;
    let ost_idx = ost.index();

    let mut enc = ff::codec::context::Context::new_with_codec(codec)
        .encoder().audio()
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Encoder: {}", e)))?;

    let dst_fmt = ff::util::format::Sample::I16(ff::util::format::sample::Type::Packed);
    enc.set_rate(target_rate as i32);
    enc.set_channel_layout(dst_layout);
    enc.set_format(dst_fmt);
    enc.set_time_base((1, target_rate as i32));

    let mut encoder = enc.open_as(codec)
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Encoder open: {}", e)))?;

    ost.set_parameters(&encoder);
    out_ctx.write_header()
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Write header: {}", e)))?;

    // Create resampler
    let mut resampler = ff::software::resampling::Context::get(
        src_fmt, src_layout, src_rate,
        dst_fmt, dst_layout, target_rate,
    ).map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Resampler: {}", e)))?;

    let out_tb = out_ctx.stream(ost_idx).map(|s| s.time_base())
        .unwrap_or(ff::Rational::new(1, target_rate as i32));
    let enc_tb = ff::Rational::new(1, target_rate as i32);

    // Process packets
    for pkt in packets {
        decoder.send_packet(pkt).ok();
        
        loop {
            let mut decoded = ff::frame::Audio::empty();
            match decoder.receive_frame(&mut decoded) {
                Ok(_) => {
                    fix_channel_layout(&mut decoded);
                    
                    let mut resampled = ff::frame::Audio::empty();
                    if resampler.run(&decoded, &mut resampled).is_ok() && resampled.samples() > 0 {
                        encoder.send_frame(&resampled).ok();
                        drain_encoder(&mut encoder, out_ctx, ost_idx, enc_tb, out_tb);
                    }
                }
                Err(ff::Error::Other { errno }) if errno == EAGAIN => break,
                Err(_) => break,
            }
        }
    }

    // Flush
    decoder.send_eof().ok();
    loop {
        let mut decoded = ff::frame::Audio::empty();
        match decoder.receive_frame(&mut decoded) {
            Ok(_) => {
                let mut resampled = ff::frame::Audio::empty();
                if resampler.run(&decoded, &mut resampled).is_ok() && resampled.samples() > 0 {
                    encoder.send_frame(&resampled).ok();
                    drain_encoder(&mut encoder, out_ctx, ost_idx, enc_tb, out_tb);
                }
            }
            Err(ff::Error::Other { errno }) if errno == EAGAIN => break,
            Err(_) => break,
        }
    }

    // Flush resampler
    let mut resampled = ff::frame::Audio::empty();
    while resampler.flush(&mut resampled).is_ok() && resampled.samples() > 0 {
        encoder.send_frame(&resampled).ok();
        drain_encoder(&mut encoder, out_ctx, ost_idx, enc_tb, out_tb);
        resampled = ff::frame::Audio::empty();
    }

    // Flush encoder
    encoder.send_eof().ok();
    drain_encoder(&mut encoder, out_ctx, ost_idx, enc_tb, out_tb);

    Ok(())
}

/// Encode to AAC (M4A/MP4)
fn encode_aac(
    packets: &[ff::Packet],
    decoder: &mut ff::decoder::Audio,
    src_fmt: ff::util::format::Sample,
    src_layout: ff::channel_layout::ChannelLayout,
    src_rate: u32,
    out_ctx: &mut ff::format::context::Output,
    target_rate: u32,
    dst_layout: ff::channel_layout::ChannelLayout,
    channels: usize,
) -> MediaResult<()> {
    let codec = ff::encoder::find(ff::codec::Id::AAC)
        .ok_or_else(|| MediaError::new(MediaErrorCode::AudioResampleFailed, "AAC encoder not found"))?;

    let mut ost = out_ctx.add_stream(codec)
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Add stream: {}", e)))?;
    let ost_idx = ost.index();

    let mut enc = ff::codec::context::Context::new_with_codec(codec)
        .encoder().audio()
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Encoder: {}", e)))?;

    let dst_fmt = ff::util::format::Sample::F32(ff::util::format::sample::Type::Planar);
    enc.set_rate(target_rate as i32);
    enc.set_channel_layout(dst_layout);
    enc.set_format(dst_fmt);
    enc.set_time_base((1, target_rate as i32));
    enc.set_bit_rate(192_000);

    let mut encoder = enc.open_as(codec)
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Encoder open: {}", e)))?;

    let frame_size = encoder.frame_size() as usize;
    let frame_size = if frame_size == 0 { 1024 } else { frame_size };

    ost.set_parameters(&encoder);
    out_ctx.write_header()
        .map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Write header: {}", e)))?;

    // Create resampler
    let mut resampler = ff::software::resampling::Context::get(
        src_fmt, src_layout, src_rate,
        dst_fmt, dst_layout, target_rate,
    ).map_err(|e| MediaError::new(MediaErrorCode::AudioResampleFailed, format!("Resampler: {}", e)))?;

    let out_tb = out_ctx.stream(ost_idx).map(|s| s.time_base())
        .unwrap_or(ff::Rational::new(1, target_rate as i32));
    let enc_tb = ff::Rational::new(1, target_rate as i32);

    // Sample buffer for AAC frame alignment with O(n) read-offset tracking
    let mut buffer = SampleBuffer::new(channels, 65536);
    let mut pts: i64 = 0;

    // Process packets
    for pkt in packets {
        decoder.send_packet(pkt).ok();

        loop {
            let mut decoded = ff::frame::Audio::empty();
            match decoder.receive_frame(&mut decoded) {
                Ok(_) => {
                    fix_channel_layout(&mut decoded);

                    // Pre-allocate output frame for proper format and channels
                    let mut resampled = ff::frame::Audio::new(
                        dst_fmt,
                        decoded.samples().checked_mul(2).ok_or_else(|| MediaError::new(MediaErrorCode::AudioResampleFailed, "sample count overflow"))?,
                        dst_layout,
                    );

                    if resampler.run(&decoded, &mut resampled).is_ok() && resampled.samples() > 0 {
                        // Copy samples to buffer - check data is valid
                        let samples = resampled.samples();
                        for ch in 0..channels {
                            let data = resampled.data(ch);
                            if !data.is_empty() && data.len() >= samples * std::mem::size_of::<f32>() {
                                let ptr = data.as_ptr();
                                // SAFETY: We verified data is non-empty and has sufficient length for `samples` f32 values.
                                // The pointer comes from a valid FFmpeg audio frame buffer.
                                debug_assert!(ch < channels, "channel index {} out of bounds {}", ch, channels);
                                debug_assert!(!ptr.is_null(), "audio data pointer is null");
                                debug_assert!((ptr as usize).is_multiple_of(std::mem::align_of::<f32>()), "audio data pointer is not aligned for f32");
                                debug_assert!(data.len() >= samples * std::mem::size_of::<f32>(), "data length {} insufficient for {} samples", data.len(), samples);
                                let slice: &[f32] = unsafe {
                                    std::slice::from_raw_parts(ptr as *const f32, samples)
                                };
                                buffer.extend(ch, slice);
                            }
                        }

                        // Encode complete frames
                        while buffer.len(0) >= frame_size {
                            let mut frame = ff::frame::Audio::new(dst_fmt, frame_size, dst_layout);
                            frame.set_pts(Some(pts));
                            pts += frame_size as i64;

                            for ch in 0..channels {
                                let dst = frame.data_mut(ch);
                                let bytes = frame_size * std::mem::size_of::<f32>();
                                let src_ptr = buffer.as_ptr(ch);
                                // SAFETY: buffer has at least frame_size elements at read_pos,
                                // so the pointer is valid for `bytes` bytes. We reinterpret f32 as u8 for memcpy.
                                debug_assert!(!src_ptr.is_null(), "buffer pointer is null");
                                debug_assert!(buffer.len(ch) >= frame_size, "buffer[{}] length {} < frame_size {}", ch, buffer.len(ch), frame_size);
                                debug_assert!(dst.len() >= bytes, "destination buffer too small: {} < {}", dst.len(), bytes);
                                dst[..bytes].copy_from_slice(unsafe {
                                    std::slice::from_raw_parts(src_ptr as *const u8, bytes)
                                });
                            }

                            // O(1) consume: advance read_pos, compact periodically
                            buffer.consume(frame_size);

                            encoder.send_frame(&frame).ok();
                            drain_encoder(&mut encoder, out_ctx, ost_idx, enc_tb, out_tb);
                        }
                    }
                }
                Err(ff::Error::Other { errno }) if errno == EAGAIN => break,
                Err(_) => break,
            }
        }
    }

    // Flush decoder - use pre-allocated frame
    decoder.send_eof().ok();
    loop {
        let mut decoded = ff::frame::Audio::empty();
        match decoder.receive_frame(&mut decoded) {
            Ok(_) => {
                let mut resampled = ff::frame::Audio::new(dst_fmt, decoded.samples().checked_mul(2).ok_or_else(|| MediaError::new(MediaErrorCode::AudioResampleFailed, "sample count overflow"))?, dst_layout);
                if resampler.run(&decoded, &mut resampled).is_ok() && resampled.samples() > 0 {
                    let samples = resampled.samples();
                    for ch in 0..channels {
                        let data = resampled.data(ch);
                        if !data.is_empty() && data.len() >= samples * std::mem::size_of::<f32>() {
                            let ptr = data.as_ptr();
                            debug_assert!(ch < channels, "channel index {} out of bounds {}", ch, channels);
                            debug_assert!(!ptr.is_null(), "audio data pointer is null");
                            debug_assert!((ptr as usize).is_multiple_of(std::mem::align_of::<f32>()), "audio data pointer is not aligned for f32");
                            debug_assert!(data.len() >= samples * std::mem::size_of::<f32>(), "data length {} insufficient for {} samples", data.len(), samples);
                            let slice: &[f32] = unsafe {
                                std::slice::from_raw_parts(ptr as *const f32, samples)
                            };
                            buffer.extend(ch, slice);
                        }
                    }
                }
            }
            Err(ff::Error::Other { errno }) if errno == EAGAIN => break,
            Err(_) => break,
        }
    }

    // Flush resampler - use pre-allocated frame
    let mut resampled = ff::frame::Audio::new(dst_fmt, 4096, dst_layout);
    while resampler.flush(&mut resampled).is_ok() && resampled.samples() > 0 {
        let samples = resampled.samples();
        for ch in 0..channels {
            let data = resampled.data(ch);
            if !data.is_empty() && data.len() >= samples * std::mem::size_of::<f32>() {
                let ptr = data.as_ptr();
                debug_assert!(ch < channels, "channel index {} out of bounds {}", ch, channels);
                debug_assert!(!ptr.is_null(), "audio data pointer is null");
                debug_assert!((ptr as usize).is_multiple_of(std::mem::align_of::<f32>()), "audio data pointer is not aligned for f32");
                debug_assert!(data.len() >= samples * std::mem::size_of::<f32>(), "data length {} insufficient for {} samples", data.len(), samples);
                let slice: &[f32] = unsafe {
                    std::slice::from_raw_parts(ptr as *const f32, samples)
                };
                buffer.extend(ch, slice);
            }
        }
        resampled = ff::frame::Audio::new(dst_fmt, 4096, dst_layout);
    }

    // Compact any remaining consumed prefix before final encode
    buffer.compact();

    // Encode remaining samples with padding
    if !buffer.channels[0].is_empty() {
        for ch in 0..channels {
            buffer.resize(ch, frame_size);
        }

        let mut frame = ff::frame::Audio::new(dst_fmt, frame_size, dst_layout);
        frame.set_pts(Some(pts));

        for ch in 0..channels {
            let dst = frame.data_mut(ch);
            let bytes = frame_size * std::mem::size_of::<f32>();
            let src_ptr = buffer.as_ptr(ch);
            debug_assert!(!src_ptr.is_null(), "buffer pointer is null");
            debug_assert!(buffer.len(ch) == frame_size, "buffer[{}] length {} != frame_size {}", ch, buffer.len(ch), frame_size);
            debug_assert!(dst.len() >= bytes, "destination buffer too small: {} < {}", dst.len(), bytes);
            dst[..bytes].copy_from_slice(unsafe {
                std::slice::from_raw_parts(src_ptr as *const u8, bytes)
            });
        }

        encoder.send_frame(&frame).ok();
        drain_encoder(&mut encoder, out_ctx, ost_idx, enc_tb, out_tb);
    }

    // Flush encoder
    encoder.send_eof().ok();
    drain_encoder(&mut encoder, out_ctx, ost_idx, enc_tb, out_tb);

    Ok(())
}

/// Fix empty channel layout on decoded frame
fn fix_channel_layout(frame: &mut ff::frame::Audio) {
    if frame.channel_layout().is_empty() {
        let layout = match frame.channels() {
            1 => ff::channel_layout::ChannelLayout::MONO,
            2 => ff::channel_layout::ChannelLayout::STEREO,
            _ => ff::channel_layout::ChannelLayout::STEREO,
        };
        frame.set_channel_layout(layout);
    }
}

/// Drain encoder packets to output
fn drain_encoder(
    encoder: &mut ff::encoder::Audio,
    out_ctx: &mut ff::format::context::Output,
    stream_idx: usize,
    enc_tb: ff::Rational,
    out_tb: ff::Rational,
) {
    loop {
        let mut pkt = ff::Packet::empty();
        match encoder.receive_packet(&mut pkt) {
            Ok(_) => {
                pkt.set_stream(stream_idx);
                pkt.rescale_ts(enc_tb, out_tb);
                let _ = pkt.write_interleaved(out_ctx);
            }
            Err(ff::Error::Other { errno }) if errno == EAGAIN => break,
            Err(_) => break,
        }
    }
}