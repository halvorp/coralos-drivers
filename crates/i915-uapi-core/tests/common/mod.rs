// SPDX-License-Identifier: GPL-2.0-only
//! Shared test scaffolding: the 0x591C device, an in-memory ExecEnv and a flattened EXECBUFFER2 builder.
#![allow(dead_code)]
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_devinfo_core::lookup;
use i915_uapi_core::codec::Writer;
use i915_uapi_core::types::{ExecFence, ExecObject2, ExtensionHeader, RelocationEntry};
use i915_uapi_core::{ContextInfo, ExecEnv, ObjectInfo, SyncobjInfo, UapiDevice};

/// FUSE2 with slice-enable field (27:25) `s` and subslice-disable field (23:20) `d` (intel_gt_regs.h:604-605).
pub fn fuse2(s: u32, d: u32) -> u32 {
    (s << 25) | (d << 20)
}

/// A Kaby Lake device by PCI id with a given fuse state.
pub fn kbl(devid: u16, fuse2: u32, eu_disable: [u32; 3]) -> UapiDevice {
    let desc = lookup(devid).unwrap();
    let sseu = gen9_sseu_info(fuse2, eu_disable);
    // CTC_MODE: crystal source (bit 0 = 0), shift field (2:1) = 2 -> 24 MHz >> (3 - 2) (intel_gt_clock_utils.c:102).
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal source expected") };
    UapiDevice::new(desc, 0x09, sseu, hz, 8 << 30).unwrap()
}

/// 0x591C: Kaby Lake AML GT2 (Pixelbook Go), 1 slice x 3 subslices x 8 EUs = 24 EUs.
pub fn gt2() -> UapiDevice {
    kbl(0x591c, fuse2(0b001, 0b1000), [0, 0, 0])
}

/// 0x5926: Kaby Lake ULT GT3 (two slices, two VDBOXes): 2 x 3 x 8 = 48 EUs.
pub fn gt3() -> UapiDevice {
    kbl(0x5926, fuse2(0b011, 0b1000), [0, 0, 0])
}

pub struct Env {
    pub contexts: Vec<(u32, ContextInfo)>,
    pub objects: Vec<(u32, ObjectInfo)>,
    pub syncobjs: Vec<(u32, SyncobjInfo)>,
    pub sync_files: Vec<u32>,
}

impl Env {
    /// Context 1 (legacy rings), objects 10.. of 1 MiB, no syncobjs.
    pub fn basic() -> Env {
        Env {
            contexts: vec![(1, ContextInfo { user_engines: None, user_engine_present: true })],
            objects: (10..30).map(|h| (h, ObjectInfo { size: 1 << 20, bound_offset: None })).collect(),
            syncobjs: vec![],
            sync_files: vec![],
        }
    }
}

impl ExecEnv for Env {
    fn context(&self, id: u32) -> Option<ContextInfo> {
        self.contexts.iter().find(|c| c.0 == id).map(|c| c.1)
    }
    fn object(&self, handle: u32) -> Option<ObjectInfo> {
        self.objects.iter().find(|c| c.0 == handle).map(|c| c.1)
    }
    fn syncobj(&self, handle: u32) -> Option<SyncobjInfo> {
        self.syncobjs.iter().find(|c| c.0 == handle).map(|c| c.1)
    }
    fn sync_file(&self, token: u32) -> bool {
        self.sync_files.contains(&token)
    }
}

/// One extension record to put on the wire.
pub struct TimelineExt {
    pub header: ExtensionHeader,
    pub fences: Vec<ExecFence>,
    pub values: Vec<u64>,
    /// Override the declared fence_count (default: fences.len()).
    pub declared_count: Option<u64>,
}

impl TimelineExt {
    pub fn new(fences: Vec<ExecFence>, values: Vec<u64>) -> TimelineExt {
        TimelineExt { header: ExtensionHeader::default(), fences, values, declared_count: None }
    }
}

/// A flattened EXECBUFFER2 request.
pub struct Exec {
    pub flags: u64,
    pub rsvd1: u64,
    pub rsvd2: u64,
    pub start: u32,
    pub len: u32,
    pub dr1: u32,
    pub dr4: u32,
    pub num_cliprects: Option<u32>,
    pub cliprects_present: u32,
    pub objects: Vec<ExecObject2>,
    pub relocs: Vec<RelocationEntry>,
    pub reloc_total: Option<u32>,
    pub fences: Vec<ExecFence>,
    pub exts: Vec<TimelineExt>,
    pub ext_count: Option<u32>,
    pub buffer_count: Option<u32>,
}

impl Exec {
    pub fn new(flags: u64, objects: Vec<ExecObject2>) -> Exec {
        Exec {
            flags,
            rsvd1: 1,
            rsvd2: 0,
            start: 0,
            len: 0,
            dr1: 0,
            dr4: 0,
            num_cliprects: None,
            cliprects_present: 0,
            objects,
            relocs: vec![],
            reloc_total: None,
            fences: vec![],
            exts: vec![],
            ext_count: None,
            buffer_count: None,
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let mut buf = vec![0u8; 1 << 20];
        let n = {
            let mut w = Writer::new(&mut buf);
            w.u64(self.flags).unwrap();
            w.u64(self.rsvd1).unwrap();
            w.u64(self.rsvd2).unwrap();
            w.u32(self.buffer_count.unwrap_or(self.objects.len() as u32)).unwrap();
            w.u32(self.start).unwrap();
            w.u32(self.len).unwrap();
            w.u32(self.dr1).unwrap();
            w.u32(self.dr4).unwrap();
            w.u32(self.num_cliprects.unwrap_or(self.fences.len() as u32)).unwrap();
            w.u32(self.cliprects_present).unwrap();
            w.u32(self.reloc_total.unwrap_or(self.relocs.len() as u32)).unwrap();
            w.u32(self.ext_count.unwrap_or(self.exts.len() as u32)).unwrap();
            w.u32(0).unwrap();
            for o in &self.objects {
                o.encode(&mut w).unwrap();
            }
            for r in &self.relocs {
                r.encode(&mut w).unwrap();
            }
            for f in &self.fences {
                f.encode(&mut w).unwrap();
            }
            for e in &self.exts {
                e.header.encode(&mut w).unwrap();
                w.u64(e.declared_count.unwrap_or(e.fences.len() as u64)).unwrap();
                for f in &e.fences {
                    f.encode(&mut w).unwrap();
                }
                for v in &e.values {
                    w.u64(*v).unwrap();
                }
            }
            w.pos()
        };
        buf.truncate(n);
        buf
    }
}

/// A softpinned iris-style object: `SUPPORTS_48B_ADDRESS | PINNED | ASYNC` at `offset` (iris_kmd_backend.c:346-350).
pub fn pinned(handle: u32, offset: u64) -> ExecObject2 {
    use i915_uapi_core::consts::exec_object as eo;
    ExecObject2 {
        handle,
        offset,
        flags: eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS | eo::EXEC_OBJECT_PINNED | eo::EXEC_OBJECT_ASYNC,
        ..Default::default()
    }
}
