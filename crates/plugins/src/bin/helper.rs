//! Native plugin helper process for RT operations

use std::mem::{align_of, size_of};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use clap::Parser;
use shared_memory::{Shmem, ShmemConf};
use uuid::Uuid;

use racing_wheel_plugins::native::{PluginFrame, SharedMemoryHeader};

#[derive(Parser)]
#[command(name = "wheel-plugin-helper")]
#[command(about = "Helper process for native plugin RT operations")]
struct Args {
    /// Plugin ID
    #[arg(long)]
    plugin_id: Uuid,

    /// Shared memory ID
    #[arg(long)]
    shmem_id: String,

    /// Budget in microseconds
    #[arg(long)]
    budget_us: u32,
}

const HELPER_SHARED_MEMORY_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HelperMemoryLayout {
    frame_offset: usize,
    max_frames: u32,
}

impl HelperMemoryLayout {
    fn new(
        header: &SharedMemoryHeader,
        mapped_len: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if header.version != HELPER_SHARED_MEMORY_VERSION {
            return Err(format!(
                "unsupported shared memory version {}; expected {}",
                header.version, HELPER_SHARED_MEMORY_VERSION
            )
            .into());
        }

        let frame_size = header.frame_size as usize;
        if frame_size != size_of::<PluginFrame>() {
            return Err(format!(
                "shared memory frame size {} does not match PluginFrame size {}",
                frame_size,
                size_of::<PluginFrame>()
            )
            .into());
        }

        if header.max_frames == 0 {
            return Err("shared memory ring buffer capacity must be nonzero".into());
        }

        let frame_offset = size_of::<SharedMemoryHeader>();
        if !frame_offset.is_multiple_of(align_of::<PluginFrame>()) {
            return Err(format!(
                "shared memory frame offset {} is not aligned for PluginFrame alignment {}",
                frame_offset,
                align_of::<PluginFrame>()
            )
            .into());
        }

        let payload_len = frame_size
            .checked_mul(header.max_frames as usize)
            .ok_or("shared memory frame payload size overflow")?;
        let required_len = frame_offset
            .checked_add(payload_len)
            .ok_or("shared memory total size overflow")?;

        if mapped_len < required_len {
            return Err(format!(
                "shared memory mapping too small: {} bytes mapped, {} bytes required",
                mapped_len, required_len
            )
            .into());
        }

        Ok(Self {
            frame_offset,
            max_frames: header.max_frames,
        })
    }

    fn frame_index(self, sequence: u32) -> usize {
        (sequence % self.max_frames) as usize
    }
}

struct CheckedHelperMemory<'a> {
    base: *mut u8,
    header: &'a SharedMemoryHeader,
    layout: HelperMemoryLayout,
}

impl<'a> CheckedHelperMemory<'a> {
    fn from_shared_memory(shared_memory: &'a Shmem) -> Result<Self, Box<dyn std::error::Error>> {
        let mapped_len = shared_memory.len();
        if mapped_len < size_of::<SharedMemoryHeader>() {
            return Err(format!(
                "shared memory mapping too small for header: {} bytes mapped, {} bytes required",
                mapped_len,
                size_of::<SharedMemoryHeader>()
            )
            .into());
        }

        let base = shared_memory.as_ptr();
        if base.is_null() {
            return Err("shared memory mapping returned a null base pointer".into());
        }

        if !(base as usize).is_multiple_of(align_of::<SharedMemoryHeader>()) {
            return Err(format!(
                "shared memory base pointer is not aligned for SharedMemoryHeader alignment {}",
                align_of::<SharedMemoryHeader>()
            )
            .into());
        }

        // SAFETY: `Shmem::as_ptr` is stable for the lifetime of `shared_memory`;
        // the mapping is large enough for a header and aligned for this repr(C)
        // header before the reference is formed. Payload access remains guarded
        // by `HelperMemoryLayout::new`.
        let header = unsafe { &*base.cast::<SharedMemoryHeader>() };
        let layout = HelperMemoryLayout::new(header, mapped_len)?;

        let frames_addr = (base as usize)
            .checked_add(layout.frame_offset)
            .ok_or("shared memory frame base address overflow")?;
        if !frames_addr.is_multiple_of(align_of::<PluginFrame>()) {
            return Err(format!(
                "shared memory frame base is not aligned for PluginFrame alignment {}",
                align_of::<PluginFrame>()
            )
            .into());
        }

        Ok(Self {
            base,
            header,
            layout,
        })
    }

    fn frame_ptr(&self, index: usize) -> *mut PluginFrame {
        debug_assert!(index < self.layout.max_frames as usize);
        self.base
            .wrapping_add(self.layout.frame_offset)
            .cast::<PluginFrame>()
            .wrapping_add(index)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    tracing::info!(
        plugin_id = %args.plugin_id,
        shmem_id = %args.shmem_id,
        budget_us = args.budget_us,
        "Starting plugin helper process"
    );

    // Open shared memory
    let shared_memory = ShmemConf::new().os_id(&args.shmem_id).open()?;

    // Main processing loop
    let mut frame_count = 0u64;
    let mut total_processing_time = Duration::ZERO;

    loop {
        // Check shutdown flag
        let memory = CheckedHelperMemory::from_shared_memory(&shared_memory)?;
        let shutdown = memory.header.shutdown_flag.load(Ordering::Acquire);

        if shutdown {
            tracing::info!("Shutdown requested, exiting");
            break;
        }

        // Try to read a frame
        if let Some(mut frame) = read_frame_from_shared_memory(&shared_memory)? {
            let start_time = Instant::now();

            // Process the frame (simplified - real implementation would load and call plugin)
            process_frame(&mut frame, args.budget_us)?;

            let processing_time = start_time.elapsed();
            total_processing_time += processing_time;
            frame_count += 1;

            // Write result back
            write_frame_to_shared_memory(&shared_memory, frame)?;

            // Check budget violation
            if processing_time.as_micros() > args.budget_us as u128 {
                tracing::warn!(
                    processing_time_us = processing_time.as_micros(),
                    budget_us = args.budget_us,
                    "Budget violation detected"
                );
            }

            // Log statistics periodically
            if frame_count.is_multiple_of(1000) {
                let avg_time = total_processing_time / frame_count as u32;
                tracing::info!(
                    frames_processed = frame_count,
                    avg_processing_time_us = avg_time.as_micros(),
                    "Processing statistics"
                );
            }
        } else {
            // No frame available, sleep briefly
            std::thread::sleep(Duration::from_micros(100));
        }
    }

    tracing::info!(
        frames_processed = frame_count,
        total_time_ms = total_processing_time.as_millis(),
        "Helper process shutting down"
    );

    Ok(())
}

fn read_frame_from_shared_memory(
    shared_memory: &Shmem,
) -> Result<Option<PluginFrame>, Box<dyn std::error::Error>> {
    let memory = CheckedHelperMemory::from_shared_memory(shared_memory)?;
    let producer_seq = memory.header.producer_seq.load(Ordering::Acquire);
    let consumer_seq = memory.header.consumer_seq.load(Ordering::Acquire);

    // Check if data is available
    if consumer_seq >= producer_seq {
        return Ok(None);
    }

    // SAFETY: `CheckedHelperMemory` validated the mapped length, ABI version,
    // frame size, ring capacity, and frame alignment. The computed index is
    // bounded by `max_frames`, so this reads one initialized frame slot from
    // the helper SPSC region.
    let frame = unsafe { *memory.frame_ptr(memory.layout.frame_index(consumer_seq)) };

    // Update consumer sequence
    memory
        .header
        .consumer_seq
        .store(consumer_seq.wrapping_add(1), Ordering::Release);

    Ok(Some(frame))
}

fn write_frame_to_shared_memory(
    shared_memory: &Shmem,
    frame: PluginFrame,
) -> Result<(), Box<dyn std::error::Error>> {
    let memory = CheckedHelperMemory::from_shared_memory(shared_memory)?;
    let producer_seq = memory.header.producer_seq.load(Ordering::Acquire);
    let consumer_seq = memory.header.consumer_seq.load(Ordering::Acquire);

    // Check if ring buffer is full
    if producer_seq.wrapping_sub(consumer_seq) >= memory.layout.max_frames {
        return Err("Ring buffer full".into());
    }

    // SAFETY: `CheckedHelperMemory` validated the mapped length, ABI version,
    // frame size, ring capacity, and frame alignment. The computed index is
    // bounded by `max_frames`, so this writes one frame into the helper SPSC
    // region before publishing the producer sequence with Release ordering.
    unsafe {
        *memory.frame_ptr(memory.layout.frame_index(producer_seq)) = frame;
    }

    // Update producer sequence
    memory
        .header
        .producer_seq
        .store(producer_seq.wrapping_add(1), Ordering::Release);

    Ok(())
}

fn process_frame(
    frame: &mut PluginFrame,
    budget_us: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    // Simplified DSP processing - in real implementation, this would call the loaded plugin
    // For now, just apply a simple gain and add some processing delay
    frame.torque_out = frame.ffb_in * 0.95; // Slight attenuation

    // Simulate some processing time
    let target_time = Duration::from_micros((budget_us / 4) as u64); // Use 1/4 of budget
    while start_time.elapsed() < target_time {
        // Busy wait to simulate processing
        std::hint::spin_loop();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};

    fn header(version: u32, frame_size: usize, max_frames: u32) -> SharedMemoryHeader {
        SharedMemoryHeader {
            version,
            producer_seq: AtomicU32::new(0),
            consumer_seq: AtomicU32::new(0),
            frame_size: frame_size as u32,
            max_frames,
            shutdown_flag: AtomicBool::new(false),
        }
    }

    fn required_mapping_len(max_frames: u32) -> usize {
        size_of::<SharedMemoryHeader>() + (size_of::<PluginFrame>() * max_frames as usize)
    }

    #[test]
    fn helper_memory_layout_accepts_valid_header() -> Result<(), Box<dyn std::error::Error>> {
        let header = header(HELPER_SHARED_MEMORY_VERSION, size_of::<PluginFrame>(), 8);

        let layout = HelperMemoryLayout::new(&header, required_mapping_len(8))?;

        assert_eq!(layout.frame_offset, size_of::<SharedMemoryHeader>());
        assert_eq!(layout.max_frames, 8);
        assert_eq!(layout.frame_index(9), 1);
        Ok(())
    }

    #[test]
    fn helper_memory_layout_rejects_wrong_version() -> Result<(), Box<dyn std::error::Error>> {
        let header = header(2, size_of::<PluginFrame>(), 8);

        let err = match HelperMemoryLayout::new(&header, required_mapping_len(8)) {
            Ok(_) => return Err("expected wrong-version header to fail".into()),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("unsupported shared memory version")
        );
        Ok(())
    }

    #[test]
    fn helper_memory_layout_rejects_wrong_frame_size() -> Result<(), Box<dyn std::error::Error>> {
        let header = header(
            HELPER_SHARED_MEMORY_VERSION,
            size_of::<PluginFrame>() + 1,
            8,
        );

        let err = match HelperMemoryLayout::new(&header, required_mapping_len(8)) {
            Ok(_) => return Err("expected wrong frame size to fail".into()),
            Err(err) => err,
        };

        assert!(err.to_string().contains("does not match PluginFrame size"));
        Ok(())
    }

    #[test]
    fn helper_memory_layout_rejects_zero_capacity() -> Result<(), Box<dyn std::error::Error>> {
        let header = header(HELPER_SHARED_MEMORY_VERSION, size_of::<PluginFrame>(), 0);

        let err = match HelperMemoryLayout::new(&header, size_of::<SharedMemoryHeader>()) {
            Ok(_) => return Err("expected zero capacity to fail".into()),
            Err(err) => err,
        };

        assert!(err.to_string().contains("capacity must be nonzero"));
        Ok(())
    }

    #[test]
    fn helper_memory_layout_rejects_short_mapping() -> Result<(), Box<dyn std::error::Error>> {
        let header = header(HELPER_SHARED_MEMORY_VERSION, size_of::<PluginFrame>(), 8);

        let err = match HelperMemoryLayout::new(&header, required_mapping_len(8) - 1) {
            Ok(_) => return Err("expected short mapping to fail".into()),
            Err(err) => err,
        };

        assert!(err.to_string().contains("mapping too small"));
        Ok(())
    }
}
