//! Sample pool for organ sample set management

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, warn, debug};

use vpo_core::{Organ, Rank, Pipe, MidiNote, SampleRate};
use crate::{SampleCache, SampleData, SampleError, SampleRef};

/// Index into the sample pool
pub type SampleIndex = usize;

/// Sample pool for a loaded organ
pub struct SamplePool {
    /// All loaded samples
    samples: Vec<SampleRef>,
    /// Mapping from (rank_id, note) to sample index
    sample_map: HashMap<(String, MidiNote), SampleIndex>,
    /// Sample cache for loading
    cache: Arc<SampleCache>,
    /// Organ sample rate
    sample_rate: SampleRate,
}

impl SamplePool {
    /// Create a new sample pool
    pub fn new(cache: Arc<SampleCache>, sample_rate: SampleRate) -> Self {
        Self {
            samples: Vec::new(),
            sample_map: HashMap::new(),
            cache,
            sample_rate,
        }
    }
    
    /// Load all samples for an organ
    pub fn load_organ(&mut self, organ: &Organ) -> Result<(), SampleError> {
        info!("Loading organ: {}", organ.name);
        
        // Count total pipes to load
        let total_pipes: usize = organ.divisions.iter()
            .flat_map(|d| d.stops.iter())
            .flat_map(|s| s.ranks.iter())
            .count();
        
        info!("Loading {} rank references", total_pipes);
        
        // TODO: Load actual samples from organ definition
        // For now, this is a placeholder
        
        Ok(())
    }
    
    /// Load samples for a single rank
    pub fn load_rank(&mut self, rank_id: &str, rank: &Rank, base_path: &PathBuf) -> Result<(), SampleError> {
        debug!("Loading rank: {}", rank.name);
        
        for pipe in &rank.pipes {
            let sample_path = base_path.join(&pipe.sample_file);
            
            match self.cache.get(&sample_path) {
                Ok(sample) => {
                    let index = self.samples.len();
                    self.samples.push(sample);
                    self.sample_map.insert((rank_id.to_string(), pipe.note), index);
                }
                Err(e) => {
                    warn!("Failed to load sample for {}/{}: {}", rank_id, pipe.note, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Get sample data for a rank and note
    pub fn get_sample(&self, rank_id: &str, note: MidiNote) -> Option<&SampleRef> {
        self.sample_map
            .get(&(rank_id.to_string(), note))
            .and_then(|&idx| self.samples.get(idx))
    }
    
    /// Get sample by index
    pub fn get_by_index(&self, index: SampleIndex) -> Option<&SampleRef> {
        self.samples.get(index)
    }
    
    /// Find sample index for a rank and note
    pub fn find_index(&self, rank_id: &str, note: MidiNote) -> Option<SampleIndex> {
        self.sample_map.get(&(rank_id.to_string(), note)).copied()
    }
    
    /// Get total number of loaded samples
    pub fn count(&self) -> usize {
        self.samples.len()
    }
    
    /// Get total memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.samples.iter()
            .map(|s| s.data.len() * std::mem::size_of::<f32>())
            .sum()
    }
    
    /// Clear all samples
    pub fn clear(&mut self) {
        self.samples.clear();
        self.sample_map.clear();
    }
}

/// Progress callback for loading
pub type LoadProgressCallback = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

/// Async organ loader
pub struct OrganLoader {
    cache: Arc<SampleCache>,
    sample_rate: SampleRate,
}

impl OrganLoader {
    pub fn new(cache: Arc<SampleCache>, sample_rate: SampleRate) -> Self {
        Self { cache, sample_rate }
    }
    
    /// Load organ with progress callback
    pub fn load_with_progress(
        &self,
        organ: &Organ,
        progress: Option<LoadProgressCallback>,
    ) -> Result<SamplePool, SampleError> {
        let mut pool = SamplePool::new(self.cache.clone(), self.sample_rate);
        
        // Collect all samples to load
        let mut to_load: Vec<(String, PathBuf)> = Vec::new();
        
        for division in &organ.divisions {
            for stop in &division.stops {
                for rank_ref in &stop.ranks {
                    // TODO: Get actual sample paths from organ definition
                    // This is placeholder logic
                }
            }
        }
        
        let total = to_load.len();
        
        for (i, (name, path)) in to_load.iter().enumerate() {
            if let Some(ref cb) = progress {
                cb(i + 1, total, name);
            }
            
            match self.cache.get(path) {
                Ok(sample) => {
                    pool.samples.push(sample);
                }
                Err(e) => {
                    warn!("Failed to load {}: {}", name, e);
                }
            }
        }
        
        info!("Loaded {} samples ({:.1} MB)", 
              pool.count(), 
              pool.memory_usage() as f64 / 1024.0 / 1024.0);
        
        Ok(pool)
    }
}
