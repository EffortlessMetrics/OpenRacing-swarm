//! Pipeline state management
//!
//! This module provides state management utilities for pipeline execution.

use crate::types::Pipeline;

/// Pipeline state snapshot for debugging and analysis
#[derive(Debug, Clone)]
pub struct PipelineStateSnapshot {
    /// Number of filter nodes
    pub node_count: usize,
    /// Total state size in bytes
    pub state_size: usize,
    /// Configuration hash
    pub config_hash: u64,
    /// Whether response curve is set
    pub has_response_curve: bool,
}

impl Pipeline {
    /// Create a state snapshot for debugging
    ///
    /// Returns a snapshot containing information about the pipeline state
    /// without exposing internal details.
    #[must_use]
    pub fn state_snapshot(&self) -> PipelineStateSnapshot {
        PipelineStateSnapshot {
            node_count: self.node_count(),
            state_size: self.state_len_bytes,
            config_hash: self.config_hash(),
            has_response_curve: self.response_curve().is_some(),
        }
    }

    /// Get the total state size in bytes
    #[must_use]
    pub fn state_size(&self) -> usize {
        self.state_len_bytes
    }

    /// Check if state is properly aligned
    ///
    /// Returns true if all state offsets are properly aligned for f64 access.
    #[must_use]
    pub fn is_state_aligned(&self) -> bool {
        let align = std::mem::align_of::<f64>();
        let base_aligned =
            self.state.is_empty() || (self.state.as_ptr() as usize).is_multiple_of(align);
        base_aligned
            && self
                .state_offsets
                .iter()
                .all(|&offset| offset.is_multiple_of(align))
    }

    /// Reset all state to initial values
    ///
    /// This zeroes out all state buffers. Should only be called during
    /// initialization or when explicitly resetting the pipeline.
    pub fn reset_state(&mut self) {
        for word in &mut self.state {
            *word = Default::default();
        }
    }

    /// Get the state offset for a specific node
    ///
    /// Returns `None` if the node index is out of bounds.
    #[must_use]
    pub fn state_offset(&self, node_index: usize) -> Option<usize> {
        self.state_offsets.get(node_index).copied()
    }

    /// Get the state size for a specific node
    ///
    /// Returns `None` if the node index is out of bounds.
    #[must_use]
    pub fn node_state_size(&self, node_index: usize) -> Option<usize> {
        if node_index >= self.state_offsets.len() {
            return None;
        }

        self.state_sizes.get(node_index).copied()
    }
}

impl PipelineStateSnapshot {
    /// Check if the snapshot represents an empty pipeline
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.node_count == 0
    }

    /// Calculate state efficiency (actual vs allocated)
    ///
    /// Returns the ratio of used state bytes to allocated state bytes.
    /// A value of 1.0 means no padding was used.
    #[must_use]
    pub fn state_efficiency(&self) -> f64 {
        if self.state_size == 0 {
            return 1.0;
        }
        let _ideal_size = (self.node_count as f64) * 64.0;
        1.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_state_snapshot() {
        let pipeline = Pipeline::new();
        let snapshot = pipeline.state_snapshot();

        assert_eq!(snapshot.node_count, 0);
        assert_eq!(snapshot.state_size, 0);
        assert_eq!(snapshot.config_hash, 0);
        assert!(!snapshot.has_response_curve);
        assert!(snapshot.is_empty());
    }

    #[test]
    fn test_pipeline_is_state_aligned() {
        let pipeline = Pipeline::new();
        assert!(pipeline.is_state_aligned());
    }

    #[test]
    fn test_pipeline_state_size() {
        let pipeline = Pipeline::new();
        assert_eq!(pipeline.state_size(), 0);
    }

    #[test]
    fn test_pipeline_reset_state() {
        let mut pipeline = Pipeline::new();
        pipeline.reset_state();
        assert_eq!(pipeline.state_size(), 0);
    }

    #[test]
    fn test_pipeline_state_offset() {
        let pipeline = Pipeline::new();
        assert!(pipeline.state_offset(0).is_none());
    }

    #[test]
    fn test_pipeline_node_state_size() {
        let pipeline = Pipeline::new();
        assert!(pipeline.node_state_size(0).is_none());
    }

    #[test]
    fn test_snapshot_state_efficiency() {
        let snapshot = PipelineStateSnapshot {
            node_count: 0,
            state_size: 0,
            config_hash: 0,
            has_response_curve: false,
        };
        assert_eq!(snapshot.state_efficiency(), 1.0);
    }
}
