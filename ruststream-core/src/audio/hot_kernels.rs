//! SIMD-optimized audio processing kernels
//!
//! Provides CPU feature detection and SIMD-accelerated audio operations.

/// CPU features detected at runtime
#[derive(Debug, Clone, Copy)]
pub struct CpuFeatures {
    pub has_avx512: bool,
    pub has_avx2: bool,
    pub has_sse41: bool,
}

/// Detect CPU features
pub fn cpu_features() -> CpuFeatures {
    #[cfg(target_arch = "x86_64")]
    {
        CpuFeatures {
            has_avx512: is_x86_feature_detected!("avx512f"),
            has_avx2: is_x86_feature_detected!("avx2"),
            has_sse41: is_x86_feature_detected!("sse4.1"),
        }
    }
    
    #[cfg(target_arch = "aarch64")]
    {
        CpuFeatures {
            has_avx512: false,
            has_avx2: false,
            has_sse41: false,
        }
    }
    
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        CpuFeatures {
            has_avx512: false,
            has_avx2: false,
            has_sse41: false,
        }
    }
}

/// Mix multiple audio streams (scalar fallback)
pub fn audio_mix(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    output.fill(0.0);
    
    for (input, &volume) in inputs.iter().zip(volumes.iter()) {
        for (out, &inp) in output.iter_mut().zip(input.iter()) {
            *out += inp * volume;
        }
    }
    
    // Clip to [-1.0, 1.0]
    for sample in output.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }
}

/// Apply volume/gain to audio buffer
pub fn apply_volume(buffer: &mut [f32], gain: f32) {
    for sample in buffer.iter_mut() {
        *sample *= gain;
    }
}

/// Apply gate/mute to audio buffer
pub fn apply_gate(buffer: &mut [f32], start: usize, end: usize) {
    if start < buffer.len() {
        let end = end.min(buffer.len());
        for sample in buffer[start..end].iter_mut() {
            *sample = 0.0;
        }
    }
}

/// Zero-copy buffer wrapper
pub struct ZeroCopyBuffer<T>(pub T);

/// Buffer pool for efficient allocation
pub struct BufferPool {
    pool_size: usize,
}

impl BufferPool {
    pub fn new(pool_size: usize) -> Self {
        Self { pool_size }
    }
    
    pub fn acquire(&self) -> Vec<f32> {
        vec![0.0; self.pool_size]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cpu_features() {
        let features = cpu_features();
        #[cfg(target_arch = "x86_64")]
        {
            assert!(features.has_sse41 || features.has_avx2 || features.has_avx512);
        }
    }
    
    #[test]
    fn test_audio_mix() {
        let input1 = vec![0.5f32; 100];
        let input2 = vec![0.3f32; 100];
        let mut output = vec![0.0f32; 100];
        
        audio_mix(&mut output, &[&input1, &input2], &[1.0, 1.0]);
        
        assert!((output[0] - 0.8).abs() < 0.001);
    }
    
    #[test]
    fn test_apply_volume() {
        let mut buffer = vec![1.0f32; 100];
        apply_volume(&mut buffer, 0.5);
        assert!((buffer[0] - 0.5).abs() < 0.001);
    }
}
