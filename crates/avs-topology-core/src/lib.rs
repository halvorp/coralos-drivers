// SPDX-License-Identifier: GPL-2.0-only
//! The AVS audio topology of Linux's Intel AVS driver as a typed graph, and the ACPI NHLT table the gateways read their SSP
//! configuration from.
//!
//! * [`blocks`] - the ASoC topology container (`snd_soc_tplg_hdr` blocks, load passes, widgets, PCMs, graph, kcontrols);
//! * [`vendor`] - vendor tuple arrays and the token walk;
//! * [`manifest`], [`path`], [`model`] - the AVS tokens (`tokens.h`): libraries, audio formats, module configs, pipeline configs,
//!   bindings, conditional paths, initial configs, NHLT configs, and the path templates (paths, pipelines, modules) of the widgets;
//! * [`nhlt`] - the NHLT table: endpoints, formats, capability blobs, and the lookups AVS makes.
//!
//! Ported from Linux `sound/soc/intel/avs/topology.c` and `topology.h`, `include/uapi/sound/intel/avs/tokens.h`,
//! `sound/soc/soc-topology.c`, `include/uapi/sound/asoc.h`, `include/acpi/nhlt.h`, `drivers/acpi/nhlt.c`, `include/acpi/actbl2.h`
//! and `sound/soc/intel/avs/path.c` (`avs_nhlt_config_or_default`) (Linux v7.0-rc6). Copyright(c) 2021-2022 Intel Corporation;
//! authors Cezary Rojewski and Amadeusz Slawinski; the NHLT code is Copyright(c) 2023-2024 Intel Corporation. The Linux files are
//! `GPL-2.0-only`; this port is distributed under GPL-2.0-only.
//!
//! The crate parses bytes the caller already holds. It owns no buffer, allocates nothing, performs no I/O. Every count in a file is
//! checked against a NAMED capacity ([`model`]), every offset against its block, and every refusal has a name ([`error`]).
//! A failed load leaves the [`topology::Topology`] unusable.

#![no_std]
#![forbid(unsafe_code)]

pub mod blocks;
pub mod error;
pub mod manifest;
pub mod model;
pub mod nhlt;
pub mod path;
pub mod pool;
pub mod tokens;
pub mod topology;
pub mod vendor;
