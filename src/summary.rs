/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! End-of-run summary: the cache's own view of what it did (policy, miss
//! ratio, tier-movement totals, tier occupancy) printed to stdout, and
//! appended as one row to a summary CSV.
//!
//! ## Why this is a separate file from the percentile CSV
//!
//! `Stats::save_latency_percentiles` (`stats.rs`) writes a *distribution*:
//! 100 rows, one per percentile, truncating whatever was at `--output-csv`
//! before. That shape has nowhere sensible to put a scalar like "total
//! demotions" — repeating it identically on all 100 rows would be the only
//! option, and a sweep over several traces would still leave one file per
//! run to stitch together afterwards.
//!
//! This writes the opposite shape: **one row per run, appended**, so a sweep
//! that invokes the binary once per (trace × design × fast-tier size) ends up
//! with a single CSV whose rows are directly comparable — which is the form
//! the aggregate demotion/promotion/eviction counts are actually wanted in.
//! Both can be used together; they answer different questions.
//!
//! Appending rather than truncating also means a sweep script needs no
//! per-run filename logic — point every run at the same `--summary-csv` and
//! the file accumulates. The header is written only when the file is new (or
//! empty), so an interrupted-and-resumed sweep doesn't grow a second header
//! mid-file.

use std::{
	fs::OpenOptions,
	io::{self, Write},
	path::Path,
};

use kwik::fmt;

use crate::{
	cache_backend::CacheReport,
	stats::OpSummary,
};

/// Column order for the summary CSV. Written as the first line of a new or
/// empty file, and never rewritten afterwards; `write_row` below must keep
/// its field order in lockstep with this.
const HEADERS: &[&str] = &[
	"trace",
	"policy",
	"clients",
	"cache_max_size",
	"fast_tier_size",
	"used_size",
	"num_objects",
	"rss",
	"hwm",
	"miss_ratio",
	"total_gets",
	"total_sets",
	"get_count",
	"get_mean_ns",
	"get_p50_ns",
	"get_p99_ns",
	"get_p99999_ns",
	"get_p100_ns",
	"get_total_bytes",
	"gets_per_sec",
	"avg_get_size",
	"set_count",
	"set_mean_ns",
	"set_p50_ns",
	"set_p99_ns",
	"set_p99999_ns",
	"set_p100_ns",
	"set_total_bytes",
	"sets_per_sec",
	"avg_set_size",
	"promotions",
	"demotions",
	"evictions",
	"fast_bytes_used",
	"fast_metadata_bytes",
	"fast_dram_total",
	"slow_bytes_used",
	"fast_objects",
	"slow_objects",
];

/// Everything that identifies and describes one benchmark run.
pub struct RunSummary<'a> {
	/// Trace file name (not the full path) — the run's identity in a sweep.
	pub trace: &'a str,
	pub clients: u32,
	pub get: OpSummary,
	pub set: OpSummary,
	pub cache: CacheReport,
}

impl RunSummary<'_> {
	/// Prints the cache-side numbers, mirroring the style of
	/// `Stats::print_get_stats`/`print_set_stats` (which cover the
	/// client-side latency numbers this deliberately doesn't repeat).
	///
	/// The tier block is skipped entirely on a non-hybrid build, where
	/// promotions/demotions don't exist as a concept — printing seven zeros
	/// there would read as "nothing moved" rather than "not applicable".
	pub fn print(&self) {
		println!("\n*** CACHE stats ***\n");

		println!("Policy:\t\t{}", self.cache.policy);

		println!(
			"Max size:\t{} ({} B)",
			fmt::memory(self.cache.max_size, Some(2)),
			fmt::number(self.cache.max_size),
		);

		println!(
			"Used size:\t{} ({} B)",
			fmt::memory(self.cache.used_size, Some(2)),
			fmt::number(self.cache.used_size),
		);

		println!("Objects:\t{}", fmt::number(self.cache.num_objects));
		println!("Miss ratio:\t{:.4}", self.cache.miss_ratio);

		println!(
			"RSS:\t\t{} (hwm {})",
			fmt::memory(self.cache.rss, Some(2)),
			fmt::memory(self.cache.hwm, Some(2)),
		);

		let Some(hybrid) = &self.cache.hybrid else {
			return;
		};

		println!("\n*** TIER stats ***\n");

		if let Some(fast_tier_bytes) = self.cache.fast_tier_bytes {
			println!(
				"Fast tier size:\t{} ({} B)",
				fmt::memory(fast_tier_bytes, Some(2)),
				fmt::number(fast_tier_bytes),
			);
		}

		println!("Promotions:\t{}", fmt::number(hybrid.promotions));
		println!("Demotions:\t{}", fmt::number(hybrid.demotions));
		println!("Evictions:\t{}", fmt::number(hybrid.evictions));

		// Object bytes and metadata are reported apart because they answer
		// different questions: `fast_bytes_used` is what the tier holds *of the
		// workload*, while the reservation is the per-object DRAM tax that
		// decides how many objects can fit at all. At 196 B/object a 4 GiB tier
		// saturates on metadata alone at ~21.9 M objects, whatever their size.
		println!(
			"\nFast tier:\t{} objects, {} ({} B) of object data",
			fmt::number(hybrid.fast_objects),
			fmt::memory(hybrid.fast_bytes_used, Some(2)),
			fmt::number(hybrid.fast_bytes_used),
		);

		println!(
			"\t\t+ {} ({} B) reserved for per-object metadata",
			fmt::memory(hybrid.fast_metadata_bytes, Some(2)),
			fmt::number(hybrid.fast_metadata_bytes),
		);

		println!(
			"\t\t= {} total DRAM",
			fmt::memory(hybrid.fast_bytes_used + hybrid.fast_metadata_bytes, Some(2)),
		);

		println!(
			"Slow tier:\t{} objects, {} ({} B)",
			fmt::number(hybrid.slow_objects),
			fmt::memory(hybrid.slow_bytes_used, Some(2)),
			fmt::number(hybrid.slow_bytes_used),
		);
	}

	/// Appends this run as one row to the CSV at `path`, writing the header
	/// first if the file is new or empty.
	///
	/// Hand-rolled rather than going through `kwik`'s `CsvWriter`: that
	/// opens with `File::create` (truncating), which is the opposite of what
	/// an accumulating sweep file needs, and its `WriteRow` trait's
	/// signature differs between this crate's `hot_fix` feature states (see
	/// `stats.rs`) — neither of which is worth working around for 27
	/// scalars.
	pub fn append_csv<P>(&self, path: P) -> io::Result<()>
	where
		P: AsRef<Path>,
	{
		let path = path.as_ref();

		// Checked before opening: `OpenOptions::append` would create the
		// file, making "did it already exist" unanswerable afterwards. A
		// zero-length existing file counts as new so an interrupted run that
		// created but never wrote the file still gets a header.
		let needs_headers = match std::fs::metadata(path) {
			Ok(metadata) => metadata.len() == 0,
			Err(_) => true,
		};

		let mut file = OpenOptions::new()
			.create(true)
			.append(true)
			.open(path)?;

		if needs_headers {
			writeln!(file, "{}", HEADERS.join(","))?;
		}

		let hybrid = self.cache.hybrid.unwrap_or_default();

		// Keep in lockstep with `HEADERS` above.
		writeln!(
			file,
			"{},{},{},{},{},{},{},{},{},{:.6},{},{},{},{:.1},{:.1},{:.1},{:.1},{:.1},{},{:.1},{:.1},{},{:.1},{:.1},{:.1},{:.1},{:.1},{},{:.1},{:.1},{},{},{},{},{},{},{},{},{}",
			self.trace,
			self.cache.policy,
			self.clients,
			self.cache.max_size,
			// Empty rather than 0 on a non-hybrid build: 0 would read as a
			// configured zero-byte fast tier.
			self.cache.fast_tier_bytes
				.map(|bytes| bytes.to_string())
				.unwrap_or_default(),
			self.cache.used_size,
			self.cache.num_objects,
			self.cache.rss,
			self.cache.hwm,
			self.cache.miss_ratio,
			self.cache.total_gets,
			self.cache.total_sets,
			self.get.count,
			self.get.mean_ns,
			self.get.p50_ns,
			self.get.p99_ns,
			self.get.p99999_ns,
			self.get.p100_ns,
			self.get.total_bytes,
			self.get.ops_per_sec,
			self.get.avg_size,
			self.set.count,
			self.set.mean_ns,
			self.set.p50_ns,
			self.set.p99_ns,
			self.set.p99999_ns,
			self.set.p100_ns,
			self.set.total_bytes,
			self.set.ops_per_sec,
			self.set.avg_size,
			hybrid.promotions,
			hybrid.demotions,
			hybrid.evictions,
			hybrid.fast_bytes_used,
			hybrid.fast_metadata_bytes,
			hybrid.fast_bytes_used + hybrid.fast_metadata_bytes,
			hybrid.slow_bytes_used,
			hybrid.fast_objects,
			hybrid.slow_objects,
		)?;

		Ok(())
	}
}
