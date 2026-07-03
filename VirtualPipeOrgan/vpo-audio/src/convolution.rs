//! Convolution reverb using FFT
//!
//! Implements partitioned convolution for efficient real-time reverb processing.

use num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex, ComplexToReal};
use std::sync::Arc;

/// Partitioned convolution reverb
pub struct ConvolutionReverb {
    /// FFT size (must be power of 2)
    fft_size: usize,
    
    /// Partition size (half of FFT size)
    partition_size: usize,
    
    /// Number of partitions
    num_partitions: usize,
    
    /// Forward FFT
    fft_forward: Arc<dyn RealToComplex<f32>>,
    
    /// Inverse FFT
    fft_inverse: Arc<dyn ComplexToReal<f32>>,
    
    /// IR partitions in frequency domain (complex)
    ir_partitions: Vec<Vec<Complex<f32>>>,
    
    /// Input buffer history in frequency domain
    input_history: Vec<Vec<Complex<f32>>>,
    
    /// Current input buffer position
    input_pos: usize,
    
    /// Accumulator for overlap-add
    accumulator: Vec<f32>,
    
    /// Output buffer
    output_buffer: Vec<f32>,
    
    /// Current position in output buffer
    output_pos: usize,
    
    /// Scratch buffers for FFT
    fft_input: Vec<f32>,
    fft_output: Vec<Complex<f32>>,
    ifft_input: Vec<Complex<f32>>,
    ifft_output: Vec<f32>,
    
    /// Wet/dry mix (0.0 = dry, 1.0 = wet)
    pub mix: f32,
    
    /// Output gain
    pub gain: f32,
}

impl ConvolutionReverb {
    /// Create a new convolution reverb with the given impulse response
    pub fn new(impulse_response: &[f32], partition_size: usize) -> Self {
        let fft_size = partition_size * 2;
        let num_partitions = (impulse_response.len() + partition_size - 1) / partition_size;
        
        let mut planner = RealFftPlanner::new();
        let fft_forward = planner.plan_fft_forward(fft_size);
        let fft_inverse = planner.plan_fft_inverse(fft_size);
        
        // Partition and transform IR
        let mut ir_partitions = Vec::with_capacity(num_partitions);
        let mut fft_input = vec![0.0; fft_size];
        let mut fft_output = vec![Complex::new(0.0, 0.0); fft_size / 2 + 1];
        
        for i in 0..num_partitions {
            let start = i * partition_size;
            let end = (start + partition_size).min(impulse_response.len());
            
            // Zero-pad input
            fft_input.fill(0.0);
            for (j, &sample) in impulse_response[start..end].iter().enumerate() {
                fft_input[j] = sample;
            }
            
            // Transform
            fft_forward.process(&mut fft_input, &mut fft_output).unwrap();
            ir_partitions.push(fft_output.clone());
        }
        
        // Initialize input history
        let input_history = (0..num_partitions)
            .map(|_| vec![Complex::new(0.0, 0.0); fft_size / 2 + 1])
            .collect();
        
        Self {
            fft_size,
            partition_size,
            num_partitions,
            fft_forward,
            fft_inverse,
            ir_partitions,
            input_history,
            input_pos: 0,
            accumulator: vec![0.0; fft_size],
            output_buffer: vec![0.0; partition_size],
            output_pos: 0,
            fft_input: vec![0.0; fft_size],
            fft_output: vec![Complex::new(0.0, 0.0); fft_size / 2 + 1],
            ifft_input: vec![Complex::new(0.0, 0.0); fft_size / 2 + 1],
            ifft_output: vec![0.0; fft_size],
            mix: 0.5,
            gain: 1.0,
        }
    }
    
    /// Load impulse response from WAV file
    pub fn from_wav(path: &std::path::Path, partition_size: usize) -> Result<Self, String> {
        let reader = hound::WavReader::open(path)
            .map_err(|e| format!("Failed to open WAV file: {}", e))?;
        
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => {
                reader.into_samples::<f32>()
                    .map(|s| s.unwrap_or(0.0))
                    .collect()
            }
            hound::SampleFormat::Int => {
                let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
                reader.into_samples::<i32>()
                    .map(|s| s.unwrap_or(0) as f32 / max_val)
                    .collect()
            }
        };
        
        // Convert to mono if stereo
        let mono: Vec<f32> = if spec.channels == 2 {
            samples.chunks(2)
                .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
                .collect()
        } else {
            samples
        };
        
        Ok(Self::new(&mono, partition_size))
    }
    
    /// Process a block of samples
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (i, &sample) in input.iter().enumerate() {
            // Add to FFT input buffer
            self.fft_input[self.output_pos] = sample;
            
            // Output from previous convolution
            let wet = self.output_buffer[self.output_pos] * self.gain;
            output[i] = sample * (1.0 - self.mix) + wet * self.mix;
            
            self.output_pos += 1;
            
            // When we have a full partition, process
            if self.output_pos >= self.partition_size {
                self.process_partition();
                self.output_pos = 0;
            }
        }
    }
    
    /// Process one partition
    fn process_partition(&mut self) {
        // Zero-pad second half
        for i in self.partition_size..self.fft_size {
            self.fft_input[i] = 0.0;
        }
        
        // Forward FFT of input
        self.fft_forward.process(&mut self.fft_input, &mut self.fft_output).unwrap();
        
        // Store in history
        self.input_history[self.input_pos].copy_from_slice(&self.fft_output);
        
        // Convolve with all partitions
        for c in self.ifft_input.iter_mut() {
            *c = Complex::new(0.0f32, 0.0f32);
        }
        
        for part_idx in 0..self.ir_partitions.len() {
            let hist_idx = (self.input_pos + self.num_partitions - part_idx) % self.num_partitions;
            let input_part: &Vec<Complex<f32>> = &self.input_history[hist_idx];
            let ir_part: &Vec<Complex<f32>> = &self.ir_partitions[part_idx];

            // Complex multiplication
            let len = self.ifft_input.len().min(input_part.len()).min(ir_part.len());
            for i in 0..len {
                self.ifft_input[i] += input_part[i] * ir_part[i];
            }
        }
        
        // Inverse FFT
        self.fft_inverse.process(&mut self.ifft_input, &mut self.ifft_output).unwrap();
        
        // Normalize
        let norm = 1.0 / self.fft_size as f32;
        
        // Overlap-add
        for i in 0..self.partition_size {
            self.output_buffer[i] = self.accumulator[i] + self.ifft_output[i] * norm;
        }
        
        // Move second half to accumulator
        for i in 0..self.partition_size {
            self.accumulator[i] = self.ifft_output[i + self.partition_size] * norm;
        }
        
        // Advance input position
        self.input_pos = (self.input_pos + 1) % self.num_partitions;
        
        // Clear FFT input for next partition
        self.fft_input[..self.partition_size].fill(0.0);
    }
    
    /// Reset the reverb state
    pub fn reset(&mut self) {
        let zero: Complex<f32> = Complex::new(0.0, 0.0);
        for hist in self.input_history.iter_mut() {
            let hist_vec: &mut Vec<Complex<f32>> = hist;
            for c in hist_vec.iter_mut() {
                *c = zero;
            }
        }
        self.accumulator.fill(0.0);
        self.output_buffer.fill(0.0);
        self.output_pos = 0;
        self.input_pos = 0;
    }
}

/// Stereo convolution reverb
pub struct StereoConvolutionReverb {
    left: ConvolutionReverb,
    right: ConvolutionReverb,
}

impl StereoConvolutionReverb {
    pub fn new(ir_left: &[f32], ir_right: &[f32], partition_size: usize) -> Self {
        Self {
            left: ConvolutionReverb::new(ir_left, partition_size),
            right: ConvolutionReverb::new(ir_right, partition_size),
        }
    }
    
    pub fn set_mix(&mut self, mix: f32) {
        self.left.mix = mix;
        self.right.mix = mix;
    }
    
    pub fn set_gain(&mut self, gain: f32) {
        self.left.gain = gain;
        self.right.gain = gain;
    }
    
    pub fn process(&mut self, input_left: &[f32], input_right: &[f32], 
                   output_left: &mut [f32], output_right: &mut [f32]) {
        self.left.process(input_left, output_left);
        self.right.process(input_right, output_right);
    }
    
    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}
