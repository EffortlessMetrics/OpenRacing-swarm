//! Core pipeline types for FFB processing
//!
//! This module provides the fundamental types used in pipeline compilation
//! and execution.

use openracing_curves::CurveLut;
use openracing_filters::Frame;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Mutex, oneshot};

const STATE_WORD_BYTES: usize = 16;

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct StateWord {
    _bytes: [u8; STATE_WORD_BYTES],
}

impl Default for StateWord {
    fn default() -> Self {
        Self {
            _bytes: [0; STATE_WORD_BYTES],
        }
    }
}

/// Function pointer type for filter nodes
///
/// Each filter node is a function that takes a mutable frame and a pointer
/// to its state data. The state pointer is guaranteed to be properly aligned
/// and points to the correct state type for the filter.
///
/// # Safety
///
/// The caller must ensure that:
/// - The state pointer points to the correct state type for this filter
/// - The state pointer is properly aligned for the state type
/// - The state memory is valid for the duration of the call
pub type FilterNodeFn = fn(&mut Frame, *mut u8);

/// Compiled filter pipeline with zero-allocation execution
///
/// A pipeline contains all the filter nodes and their state for RT-safe
/// processing. Once compiled, the pipeline can be swapped atomically
/// in the RT loop.
///
/// # RT Safety
///
/// - `process()` is RT-safe: no allocations, no syscalls, O(n) where n = node count
/// - State is stored in a pre-allocated buffer with proper alignment
/// - Pipeline swap is atomic from the RT thread's perspective
///
/// # Example
///
/// ```
/// use openracing_pipeline::{Pipeline, Frame};
///
/// let mut pipeline = Pipeline::new();
/// let mut frame = Frame::default();
/// frame.torque_out = 0.5;
///
/// // RT-safe processing
/// let result = pipeline.process(&mut frame);
/// assert!(result.is_ok());
/// ```
#[derive(Debug)]
pub struct Pipeline {
    /// Function pointers for each filter node
    pub(crate) nodes: Vec<FilterNodeFn>,
    /// State storage for all nodes (Structure of Arrays)
    pub(crate) state: Vec<StateWord>,
    /// Used bytes in the aligned state buffer.
    pub(crate) state_len_bytes: usize,
    /// Offsets into state storage for each node
    pub(crate) state_offsets: Vec<usize>,
    /// State sizes in bytes for each node
    pub(crate) state_sizes: Vec<usize>,
    /// Configuration hash for deterministic comparison
    pub(crate) config_hash: u64,
    /// Optional response curve for torque transformation (pre-computed LUT)
    /// Boxed to reduce Pipeline size in enum variants
    pub(crate) response_curve: Option<Box<CurveLut>>,
}

/// Pipeline compilation result
///
/// Contains the compiled pipeline and its configuration hash
/// for change detection.
#[derive(Debug)]
pub struct CompiledPipeline {
    /// The compiled pipeline ready for RT execution
    pub pipeline: Pipeline,
    /// Configuration hash for change detection
    pub config_hash: u64,
}

/// Pipeline compilation and execution errors
///
/// # Examples
///
/// ```
/// use openracing_pipeline::PipelineError;
///
/// let err = PipelineError::InvalidConfig("reconstruction must be 0-8".to_string());
/// assert!(err.to_string().contains("reconstruction"));
/// ```
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Invalid filter configuration
    #[error("Invalid filter configuration: {0}")]
    InvalidConfig(String),

    /// Compilation failed
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),

    /// Pipeline swap failed
    #[error("Pipeline swap failed: {0}")]
    SwapFailed(String),

    /// Non-monotonic curve points
    #[error("Non-monotonic curve points")]
    NonMonotonicCurve,

    /// Invalid filter parameters
    #[error("Invalid filter parameters: {0}")]
    InvalidParameters(String),
}

/// Internal compilation task for async compilation
#[derive(Debug)]
pub(crate) struct CompilationTask {
    /// Filter configuration to compile
    pub config: racing_wheel_schemas::entities::FilterConfig,
    /// Response channel for compilation result
    pub response_tx: oneshot::Sender<Result<CompiledPipeline, PipelineError>>,
}

/// Shared compilation task queue
pub(crate) type SharedTaskQueue = Arc<Mutex<Vec<CompilationTask>>>;

impl Pipeline {
    /// Create empty pipeline
    ///
    /// An empty pipeline passes frames through unchanged (identity transform).
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            state: Vec::new(),
            state_len_bytes: 0,
            state_offsets: Vec::new(),
            state_sizes: Vec::new(),
            config_hash: 0,
            response_curve: None,
        }
    }

    /// Create pipeline with specific configuration hash
    ///
    /// Used internally during compilation to set the deterministic hash.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_pipeline::Pipeline;
    ///
    /// let pipeline = Pipeline::with_hash(0xDEADBEEF);
    /// assert_eq!(pipeline.config_hash(), 0xDEADBEEF);
    /// assert!(pipeline.is_empty());
    /// ```
    #[must_use]
    pub fn with_hash(config_hash: u64) -> Self {
        Self {
            nodes: Vec::new(),
            state: Vec::new(),
            state_len_bytes: 0,
            state_offsets: Vec::new(),
            state_sizes: Vec::new(),
            config_hash,
            response_curve: None,
        }
    }

    /// Set the response curve for this pipeline
    ///
    /// The curve is pre-computed as a LUT at profile load time (not in RT path).
    /// This ensures zero allocations during RT processing.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_pipeline::Pipeline;
    /// use openracing_curves::CurveLut;
    ///
    /// let mut pipeline = Pipeline::new();
    /// assert!(pipeline.response_curve().is_none());
    ///
    /// pipeline.set_response_curve(CurveLut::linear());
    /// assert!(pipeline.response_curve().is_some());
    /// ```
    pub fn set_response_curve(&mut self, curve: CurveLut) {
        self.response_curve = Some(Box::new(curve));
    }

    /// Get the response curve if set
    #[must_use]
    pub fn response_curve(&self) -> Option<&CurveLut> {
        self.response_curve.as_deref()
    }

    /// Get the configuration hash for this pipeline
    ///
    /// The hash changes when the pipeline configuration changes, enabling
    /// efficient change detection.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_pipeline::Pipeline;
    ///
    /// let p1 = Pipeline::new();
    /// let p2 = Pipeline::with_hash(0xCAFE);
    ///
    /// assert_eq!(p1.config_hash(), 0);
    /// assert_eq!(p2.config_hash(), 0xCAFE);
    /// assert_ne!(p1.config_hash(), p2.config_hash());
    /// ```
    #[must_use]
    pub fn config_hash(&self) -> u64 {
        self.config_hash
    }

    /// Check if pipeline is empty
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_pipeline::Pipeline;
    ///
    /// let pipeline = Pipeline::new();
    /// assert!(pipeline.is_empty());
    /// assert_eq!(pipeline.node_count(), 0);
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get the number of filter nodes
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_pipeline::Pipeline;
    ///
    /// let pipeline = Pipeline::new();
    /// assert_eq!(pipeline.node_count(), 0);
    /// assert!(pipeline.is_empty());
    /// ```
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Add a typed filter node to the pipeline (used during compilation)
    ///
    /// This method is used internally by the compiler to build the pipeline.
    /// It stores node state in a 16-byte aligned backing buffer and records the
    /// exact byte offset/size used by the executor.
    pub(crate) fn add_state_node<T>(&mut self, node_fn: FilterNodeFn, initial_state: T)
    where
        T: Copy,
    {
        let state_size = std::mem::size_of::<T>();
        let state_align = std::mem::align_of::<T>().max(std::mem::align_of::<f64>());
        assert!(
            state_align <= std::mem::align_of::<StateWord>(),
            "state type alignment exceeds pipeline state buffer alignment"
        );

        let aligned_offset = align_up(self.state_len_bytes, state_align);
        assert!(
            state_size <= usize::MAX - aligned_offset,
            "pipeline state buffer size overflow"
        );
        let end = aligned_offset + state_size;
        self.resize_state_bytes(end);

        let state_ptr = self.state_ptr_at(aligned_offset).cast::<T>();

        // SAFETY: `state_ptr` points inside the aligned state buffer after it
        // has been resized to cover `end`. `aligned_offset` is rounded to
        // `align_of::<T>()`, and the 16-byte backing storage is at least as
        // aligned as every accepted state type. `T: Copy`, so writing the value
        // into zeroed byte storage cannot skip a destructor.
        unsafe {
            state_ptr.write(initial_state);
        }

        self.nodes.push(node_fn);
        self.state_offsets.push(aligned_offset);
        self.state_sizes.push(state_size);
    }

    pub(crate) fn node_state_ptr(&mut self, node_index: usize) -> *mut u8 {
        let Some(&offset) = self.state_offsets.get(node_index) else {
            return std::ptr::null_mut();
        };
        let Some(&state_size) = self.state_sizes.get(node_index) else {
            return std::ptr::null_mut();
        };
        let Some(end) = offset.checked_add(state_size) else {
            return std::ptr::null_mut();
        };
        if end > self.state_len_bytes {
            return std::ptr::null_mut();
        }

        self.state_ptr_at(offset)
    }

    fn resize_state_bytes(&mut self, len_bytes: usize) {
        let word_count = len_bytes.div_ceil(STATE_WORD_BYTES);
        self.state.resize(word_count, StateWord::default());
        self.state_len_bytes = len_bytes;
    }

    fn state_ptr_at(&mut self, offset: usize) -> *mut u8 {
        debug_assert!(offset <= self.state_len_bytes, "state offset out of bounds");
        // SAFETY: `offset` is checked by callers against `state_len_bytes`, and
        // `resize_state_bytes` keeps enough `StateWord` entries allocated to
        // cover every byte up to `state_len_bytes`.
        unsafe { self.state.as_mut_ptr().cast::<u8>().add(offset) }
    }
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "alignment must be a power of two");
    let mask = align - 1;
    assert!(
        value <= usize::MAX - mask,
        "pipeline state buffer size overflow"
    );
    (value + mask) & !mask
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Pipeline {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            state: self.state.clone(),
            state_len_bytes: self.state_len_bytes,
            state_offsets: self.state_offsets.clone(),
            state_sizes: self.state_sizes.clone(),
            config_hash: self.config_hash,
            response_curve: self.response_curve.clone(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_new() {
        let pipeline = Pipeline::new();
        assert!(pipeline.is_empty());
        assert_eq!(pipeline.node_count(), 0);
        assert_eq!(pipeline.config_hash(), 0);
    }

    #[test]
    fn test_pipeline_with_hash() {
        let hash = 0xDEADBEEF_u64;
        let pipeline = Pipeline::with_hash(hash);
        assert_eq!(pipeline.config_hash(), hash);
        assert!(pipeline.is_empty());
    }

    #[test]
    fn test_pipeline_response_curve() {
        let mut pipeline = Pipeline::new();
        assert!(pipeline.response_curve().is_none());

        let lut = CurveLut::linear();
        pipeline.set_response_curve(lut);
        assert!(pipeline.response_curve().is_some());
    }

    #[test]
    fn test_pipeline_clone() {
        let pipeline = Pipeline::with_hash(0x12345678);
        let cloned = pipeline.clone();
        assert_eq!(pipeline.config_hash(), cloned.config_hash());
    }

    fn no_op_node(_frame: &mut Frame, _state: *mut u8) {}

    #[repr(align(16))]
    #[derive(Clone, Copy)]
    struct AlignedState {
        _value: u8,
    }

    #[test]
    fn test_add_state_node_records_aligned_offsets_and_sizes() {
        let mut pipeline = Pipeline::new();
        pipeline.add_state_node(no_op_node, 1_u8);
        pipeline.add_state_node(no_op_node, AlignedState { _value: 7 });

        assert_eq!(pipeline.node_count(), 2);
        assert_eq!(pipeline.state_offset(0), Some(0));
        assert_eq!(pipeline.node_state_size(0), Some(std::mem::size_of::<u8>()));
        assert!(
            matches!(pipeline.state_offset(1), Some(offset) if offset.is_multiple_of(16)),
            "16-byte aligned state should be placed at a 16-byte offset"
        );
        assert_eq!(
            pipeline.node_state_size(1),
            Some(std::mem::size_of::<AlignedState>())
        );
        assert!(pipeline.state_size() >= std::mem::size_of::<u8>() + 16);
        assert!(!pipeline.node_state_ptr(0).is_null());
        assert!(!pipeline.node_state_ptr(1).is_null());
        assert!(pipeline.node_state_ptr(2).is_null());
    }
}
