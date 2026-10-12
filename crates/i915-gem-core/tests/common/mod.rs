// SPDX-License-Identifier: GPL-2.0-only
//! Shared scaffolding: the 0x591C device, a recording effect sink, fake page / table-page / sync sources, a REAL
//! `i915_submit_core::Engine` behind `EngineSet`, and a flattened EXECBUFFER2 builder.
#![allow(dead_code)]
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::lookup;
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_gem_core::*;
use i915_gt_core::emit::FlushQuirks;
use i915_gt_core::engine::{engines_for, EngineClass};
use i915_memory_core::{BindPlan, MemError, PpgttInit, TablePage, TablePageRequest, TablePageSource, UnbindPlan, Write};
use i915_submit_core::engine::{ContextSpec, Engine, EngineConfig};
use i915_submit_core::{BatchSpec, RequestRef, SubmissionPlan, SubmitError};
use i915_uapi_core::codec::Writer;
use i915_uapi_core::consts::exec_object as eo;
use i915_uapi_core::types::{ExecFence, ExecObject2, ExtensionHeader, RelocationEntry};
use i915_uapi_core::{decode_execbuf2, ExecLimits, ExecbufRequest, SyncobjInfo, UapiDevice};
use std::collections::HashMap;

pub const MIB: u64 = 1 << 20;
pub const GIB: u64 = 1 << 30;

/// 0x591C: Kaby Lake AML GT2 (Pixelbook Go), 1 slice x 3 subslices x 8 EUs.
pub fn dev() -> UapiDevice {
    let desc = lookup(0x591c).unwrap();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    UapiDevice::new(desc, 0x09, sseu, hz, 8 * GIB).unwrap()
}

pub fn gem<const V: usize, const T: usize>() -> Gem<V, T> {
    Gem::new(dev(), GemConfig { pat_enabled: true })
}

// ---- a recording sink ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Rec {
    Bind { vm: VmId, obj: ObjId, offset: u64, evicted: Vec<i915_memory_core::VmaId>, writes: Vec<Write>, retire_waits: usize, ops: usize, new_tables: u32 },
    Unbind { vm: VmId, ops: usize },
    Eff(Effect),
}

#[derive(Default)]
pub struct Sink {
    pub log: Vec<Rec>,
}

impl EffectSink for Sink {
    fn bind(&mut self, vm: VmId, obj: ObjId, plan: &BindPlan<'_>) {
        self.log.push(Rec::Bind {
            vm,
            obj,
            offset: plan.offset(),
            evicted: plan.evicted().to_vec(),
            writes: if plan.ops().iter().map(|o| o.entries() as u64).sum::<u64>() <= 2048 { plan.writes().collect() } else { vec![] },
            retire_waits: plan.ops().iter().filter(|o| matches!(o, i915_memory_core::Op::AwaitRetire { .. })).count(),
            ops: plan.ops().len(),
            new_tables: plan.new_tables(),
        });
    }
    fn unbind(&mut self, vm: VmId, plan: &UnbindPlan) {
        self.log.push(Rec::Unbind { vm, ops: plan.ops().len() });
    }
    fn effect(&mut self, e: Effect) {
        self.log.push(Rec::Eff(e));
    }
}

impl Sink {
    pub fn binds(&self) -> Vec<&Rec> {
        self.log.iter().filter(|r| matches!(r, Rec::Bind { .. })).collect()
    }
    pub fn effects(&self) -> Vec<Effect> {
        self.log.iter().filter_map(|r| if let Rec::Eff(e) = r { Some(*e) } else { None }).collect()
    }
    pub fn count_unbinds(&self) -> usize {
        self.log.iter().filter(|r| matches!(r, Rec::Unbind { .. })).count()
    }
    pub fn clear(&mut self) {
        self.log.clear();
    }
}

// ---- pages --------------------------------------------------------------------------------------------------------------------

/// Backing pages handed out from a counter; remembers every request and can be told to refuse.
pub struct Pages {
    pub next: u64,
    pub map: HashMap<ObjId, Vec<u64>>,
    pub asked: Vec<PageRequest>,
    pub refuse: bool,
}

impl Pages {
    pub fn new() -> Pages {
        Pages { next: 0x1000_0000, map: HashMap::new(), asked: vec![], refuse: false }
    }
}

impl PageSource for Pages {
    fn alloc_pages(&mut self, req: PageRequest) -> Result<(), PagesRefused> {
        if self.refuse {
            return Err(PagesRefused);
        }
        self.asked.push(req);
        if !self.map.contains_key(&req.obj) {
            let n = req.bytes / 4096;
            let v: Vec<u64> = (0..n).map(|i| self.next + i * 4096).collect();
            self.next += n * 4096;
            self.map.insert(req.obj, v);
        }
        Ok(())
    }
    fn pages(&self, obj: ObjId) -> Option<&[u64]> {
        self.map.get(&obj).map(|v| v.as_slice())
    }
}

/// Table pages from a counter (the memory-core test pool).
pub struct Pool {
    pub next: u64,
    pub asked: usize,
}

impl Pool {
    pub fn new() -> Pool {
        Pool { next: 0x4000_0000, asked: 0 }
    }
    pub fn page(&mut self) -> u64 {
        let a = self.next;
        self.next += 0x1000;
        a
    }
    pub fn init(&mut self) -> PpgttInit {
        PpgttInit { pml4: self.page(), scratch: [self.page(), self.page(), self.page(), self.page()] }
    }
}

impl TablePageSource for Pool {
    fn table_page(&mut self, _req: TablePageRequest) -> Result<TablePage, MemError> {
        self.asked += 1;
        Ok(TablePage { device_addr: self.page() })
    }
}

#[derive(Default)]
pub struct Sync {
    pub syncobjs: Vec<(u32, SyncobjInfo)>,
    pub files: Vec<u32>,
}

impl SyncTables for Sync {
    fn syncobj(&self, handle: u32) -> Option<SyncobjInfo> {
        self.syncobjs.iter().find(|s| s.0 == handle).map(|s| s.1)
    }
    fn sync_file(&self, token: u32) -> bool {
        self.files.contains(&token)
    }
}

// ---- real engines behind EngineSet --------------------------------------------------------------------------------------------

/// The render and video engines of a KBL GT2 (`i915_submit_core::Engine` each), with one ring buffer per hardware context.
pub struct Engines {
    pub engines: Vec<(u16, u16, Engine<4, 32>)>,
    pub rings: HashMap<(u16, u16, u32), Vec<u32>>,
    pub specs: u32,
    pub fail_build: bool,
    /// The engine-side request table is independent of the GEM one; a bound test on the GEM table retires the engine's at once.
    pub auto_retire: bool,
}

pub const STATUS_PAGE: u32 = 0x0004_0000;

impl Engines {
    pub fn new() -> Engines {
        let d = lookup(0x591c).unwrap();
        let cfg = EngineConfig { status_page_ggtt: STATUS_PAGE, quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, wa_ctx: None, stall_bound: 1000 };
        let mut engines = vec![];
        for (class, ec) in [(0u16, EngineClass::Render), (2u16, EngineClass::VideoDecode)] {
            let info = engines_for(&d.info).unwrap().as_slice().iter().copied().find(|e| e.class == ec).unwrap();
            engines.push((class, 0u16, Engine::new(&info, 9, cfg).unwrap()));
        }
        Engines { engines, rings: HashMap::new(), specs: 0, fail_build: false, auto_retire: false }
    }

    pub fn rcs(&mut self) -> &mut Engine<4, 32> {
        &mut self.engines[0].2
    }

    fn find(&mut self, class: u16, instance: u16) -> &mut Engine<4, 32> {
        &mut self.engines.iter_mut().find(|e| e.0 == class && e.1 == instance).expect("engine wired").2
    }

    /// Create the hardware context for GEM context `ctx_id` of `session` on engine `(class, 0)` over the PPGTT of its VM, and attach it.
    pub fn attach_on<const V: usize, const T: usize>(&mut self, gem: &mut Gem<V, T>, session: SessionId, ctx_id: u32, class: u16) -> u32 {
        let vm = gem.context_vm(session, ctx_id).unwrap();
        let n = self.specs;
        self.specs += 1;
        let spec = ContextSpec {
            lrca: 0x0100_0000 + n * 0x0001_0000,
            ring_size: 4096,
            ring_ggtt: 0x0200_0000 + n * 0x1000,
            pml4: gem.vm_pml4(vm).unwrap(),
            rpcs: 0,
            hwsp_ggtt: 0x0003_0000 + n * 0x40,
            initial_breadcrumb: true,
            default_state: None,
        };
        let ectx = self.find(class, 0).create_context(&spec).unwrap();
        self.rings.insert((class, 0, ectx), vec![0xdead_beef; 1024]);
        gem.attach_engine_context(session, ctx_id, class, 0, ectx).unwrap();
        ectx
    }

    pub fn attach<const V: usize, const T: usize>(&mut self, gem: &mut Gem<V, T>, session: SessionId, ctx_id: u32) -> u32 {
        self.attach_on(gem, session, ctx_id, 0)
    }
}

impl EngineSet for Engines {
    fn build_request(&mut self, class: u16, instance: u16, engine_ctx: u32, batch: &BatchSpec) -> Result<RequestRef, SubmitError> {
        if self.fail_build {
            return Err(SubmitError::RingFull { need: 1, space: 0 });
        }
        let mut mem = self.rings.remove(&(class, instance, engine_ctx)).unwrap();
        let auto = self.auto_retire;
        let eng = self.find(class, instance);
        let r = eng.build_request(engine_ctx, &mut mem, batch);
        if let (true, Ok(rq)) = (auto, &r) {
            eng.observe_seqno(engine_ctx, rq.seqno).unwrap();
            eng.retire(engine_ctx).unwrap();
        }
        self.rings.insert((class, instance, engine_ctx), mem);
        r
    }
    fn dequeue(&mut self, class: u16, instance: u16, now: u64) -> Result<Option<SubmissionPlan>, SubmitError> {
        self.find(class, instance).dequeue(now)
    }
}

// ---- EXECBUFFER2 builder ------------------------------------------------------------------------------------------------------

pub struct Exec {
    pub flags: u64,
    pub ctx: u64,
    pub rsvd2: u64,
    pub start: u32,
    pub len: u32,
    pub objects: Vec<ExecObject2>,
    pub relocs: Vec<RelocationEntry>,
    pub fences: Vec<ExecFence>,
    pub timeline: Vec<(Vec<ExecFence>, Vec<u64>)>,
}

impl Exec {
    pub fn new(flags: u64, objects: Vec<ExecObject2>) -> Exec {
        Exec { flags, ctx: 0, rsvd2: 0, start: 0, len: 0, objects, relocs: vec![], fences: vec![], timeline: vec![] }
    }
    pub fn build(&self) -> Vec<u8> {
        let mut buf = vec![0u8; 1 << 20];
        let n = {
            let mut w = Writer::new(&mut buf);
            let mut flags = self.flags;
            if !self.timeline.is_empty() {
                flags |= i915_uapi_core::consts::exec::I915_EXEC_USE_EXTENSIONS;
            }
            if !self.fences.is_empty() {
                flags |= i915_uapi_core::consts::exec::I915_EXEC_FENCE_ARRAY;
            }
            w.u64(flags).unwrap();
            w.u64(self.ctx).unwrap();
            w.u64(self.rsvd2).unwrap();
            w.u32(self.objects.len() as u32).unwrap();
            w.u32(self.start).unwrap();
            w.u32(self.len).unwrap();
            w.u32(0).unwrap();
            w.u32(0).unwrap();
            w.u32(self.fences.len() as u32).unwrap();
            w.u32(0).unwrap();
            w.u32(self.relocs.len() as u32).unwrap();
            w.u32(self.timeline.len() as u32).unwrap();
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
            for (fences, values) in &self.timeline {
                ExtensionHeader::default().encode(&mut w).unwrap();
                w.u64(fences.len() as u64).unwrap();
                for f in fences {
                    f.encode(&mut w).unwrap();
                }
                for v in values {
                    w.u64(*v).unwrap();
                }
            }
            w.pos()
        };
        buf.truncate(n);
        buf
    }
}

/// Decode a built request and run `f` on it (the decoded request borrows the payload).
pub fn with_req<R>(e: &Exec, f: impl FnOnce(&ExecbufRequest<'_>) -> R) -> R {
    let bytes = e.build();
    let req = decode_execbuf2(&bytes, &ExecLimits::DEFAULT).expect("decodes");
    f(&req)
}

/// An iris-style softpinned object (iris_kmd_backend.c:346-350).
pub fn pinned(handle: u32, offset: u64) -> ExecObject2 {
    ExecObject2 { handle, offset, flags: eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS | eo::EXEC_OBJECT_PINNED | eo::EXEC_OBJECT_ASYNC, ..Default::default() }
}

pub fn pinned_write(handle: u32, offset: u64) -> ExecObject2 {
    let mut o = pinned(handle, offset);
    o.flags |= eo::EXEC_OBJECT_WRITE;
    o
}

pub fn free_obj(handle: u32) -> ExecObject2 {
    ExecObject2 { handle, flags: eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS, ..Default::default() }
}

/// Everything a session test needs: the GEM, a session, its default VM pool, an engine with a context attached to context 0.
pub struct Rig<const V: usize, const T: usize> {
    pub gem: Gem<V, T>,
    pub s: SessionId,
    pub sink: Sink,
    pub pages: Pages,
    pub pool: Pool,
    pub eng: Engines,
    pub sync: Sync,
}

impl<const V: usize, const T: usize> Rig<V, T> {
    pub fn new() -> Rig<V, T> {
        let mut gem = gem::<V, T>();
        let mut sink = Sink::default();
        let mut pool = Pool::new();
        let init = pool.init();
        let s = gem.open_session(SessionCaps::default(), init, &mut sink).unwrap();
        let mut eng = Engines::new();
        eng.attach(&mut gem, s, 0);
        eng.attach_on(&mut gem, s, 0, 2);
        Rig { gem, s, sink, pages: Pages::new(), pool, eng, sync: Sync::default() }
    }

    pub fn create(&mut self, size: u64) -> u32 {
        self.gem.create(self.s, size).unwrap().handle
    }

    pub fn exec(&mut self, e: &Exec, now: u64) -> Result<ExecDone, GemError> {
        let (gem, s, sync, pages, pool, eng, sink) = (&mut self.gem, self.s, &self.sync, &mut self.pages, &mut self.pool, &mut self.eng, &mut self.sink);
        let mut tables = TablesAdapter(pool);
        with_req(e, |req| gem.execbuffer2(s, sync, req, pages, &mut tables, eng, sink, now))
    }
}

/// `Pool` is both the PPGTT-init allocator and the table-page source; this adapter lets one `&mut Pool` serve as `TablePageSource`.
pub struct TablesAdapter<'a>(pub &'a mut Pool);
impl TablePageSource for TablesAdapter<'_> {
    fn table_page(&mut self, r: TablePageRequest) -> Result<TablePage, MemError> {
        self.0.table_page(r)
    }
}
