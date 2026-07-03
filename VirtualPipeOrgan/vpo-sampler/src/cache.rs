//! Sample cache for loaded samples

use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{SampleData, SampleError, load_wav};
use vpo_core::SampleRate;

/// Reference to cached sample data
pub type SampleRef = Arc<SampleData>;

/// Sample cache with automatic loading
pub struct SampleCache {
    /// Cached samples by path
    samples: RwLock<HashMap<PathBuf, SampleRef>>,
    /// Base path for samples
    base_path: PathBuf,
    /// Target sample rate for resampling
    target_rate: SampleRate,
    /// Maximum cache size in bytes (approximate)
    max_size: usize,
    /// Current cache size
    current_size: RwLock<usize>,
}

impl SampleCache {
    /// Create a new sample cache
    pub fn new(base_path: impl Into<PathBuf>, target_rate: SampleRate) -> Self {
        Self {
            samples: RwLock::new(HashMap::new()),
            base_path: base_path.into(),
            target_rate,
            max_size: 4 * 1024 * 1024 * 1024, // 4 GB default
            current_size: RwLock::new(0),
        }
    }
    
    /// Set maximum cache size in bytes
    pub fn set_max_size(&mut self, size: usize) {
        self.max_size = size;
    }
    
    /// Get a sample, loading if necessary
    pub fn get(&self, path: &Path) -> Result<SampleRef, SampleError> {
        let full_path = self.base_path.join(path);
        
        // Check cache first
        {
            let cache = self.samples.read();
            if let Some(sample) = cache.get(&full_path) {
                return Ok(sample.clone());
            }
        }
        
        // Load sample
        let mut sample = load_wav(&full_path)?;
        
        // Resample if necessary
        if sample.sample_rate != self.target_rate {
            sample = crate::resample(&sample, self.target_rate)?;
        }
        
        let sample_size = sample.data.len() * std::mem::size_of::<f32>();
        let sample_ref = Arc::new(sample);
        
        // Add to cache
        {
            let mut cache = self.samples.write();
            let mut size = self.current_size.write();
            
            // Check if we need to evict
            while *size + sample_size > self.max_size && !cache.is_empty() {
                // Simple eviction: remove first entry
                if let Some(key) = cache.keys().next().cloned() {
                    if let Some(removed) = cache.remove(&key) {
                        *size -= removed.data.len() * std::mem::size_of::<f32>();
                    }
                }
            }
            
            cache.insert(full_path, sample_ref.clone());
            *size += sample_size;
        }
        
        Ok(sample_ref)
    }
    
    /// Preload a list of samples
    pub fn preload(&self, paths: &[PathBuf]) -> Vec<Result<SampleRef, SampleError>> {
        use rayon::prelude::*;
        
        paths.par_iter()
            .map(|path| self.get(path))
            .collect()
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.samples.write();
        cache.clear();
        *self.current_size.write() = 0;
    }
    
    /// Get current cache size in bytes
    pub fn size(&self) -> usize {
        *self.current_size.read()
    }
    
    /// Get number of cached samples
    pub fn count(&self) -> usize {
        self.samples.read().len()
    }
}
