// Full workload characterisation of a 25-byte-record trace.
//
//   ts u64 | cmd u8 | key u64 | value_size u32 | ttl u32
//
// Read-through definitions, following results/twitter_trace_working_sets.md:
// an object is written once, on the get that misses, so its resident size is
// the size at the key's FIRST appearance. The get:set the cache performs is
// (1 - miss) : miss, and at infinite cache the miss floor is distinct/records.
//
// Reports two size distributions, which answer different questions:
//   per OBJECT  -- what the working set is made of; the metadata:data ratio
//   per ACCESS  -- what a request actually sees; skewed toward hot objects

use std::convert::TryInto;
use std::fs::File;
use std::io::{BufReader, Read};

#[derive(Clone, Copy)]
struct Slot {
	key: u64,
	first: u32,
	max: u32,
	count: u32,
}

struct Table {
	slots: Vec<Slot>,
	mask: usize,
	len: usize,
}

#[inline]
fn mix(mut x: u64) -> u64 {
	x ^= x >> 30;
	x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
	x ^= x >> 27;
	x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
	x ^ (x >> 31)
}

impl Table {
	fn with_capacity(cap: usize) -> Table {
		let cap = cap.next_power_of_two().max(1024);
		Table {
			slots: vec![Slot { key: 0, first: 0, max: 0, count: 0 }; cap],
			mask: cap - 1,
			len: 0,
		}
	}

	#[inline]
	fn probe(&self, key: u64) -> usize {
		let mut i = mix(key) as usize & self.mask;
		loop {
			let s = &self.slots[i];
			if s.first == 0 || s.key == key {
				return i;
			}
			i = (i + 1) & self.mask;
		}
	}

	/// True if this was the key's first appearance.
	#[inline]
	fn observe(&mut self, key: u64, size: u32) -> bool {
		if (self.len + 1) * 10 >= self.slots.len() * 7 {
			self.grow();
		}
		let i = self.probe(key);
		if self.slots[i].first == 0 {
			self.slots[i] = Slot { key, first: size, max: size, count: 1 };
			self.len += 1;
			return true;
		}
		self.slots[i].count += 1;
		if size > self.slots[i].max {
			self.slots[i].max = size;
		}
		false
	}

	fn grow(&mut self) {
		let mut b = Table::with_capacity(self.slots.len() * 2);
		for s in &self.slots {
			if s.first != 0 {
				let i = b.probe(s.key);
				b.slots[i] = *s;
				b.len += 1;
			}
		}
		*self = b;
	}
}

/// Exact counts below `SMALL`, explicit list above.
struct Hist {
	small: Vec<u64>,
	large: Vec<u32>,
	n: u64,
	sum: u128,
}

const SMALL: usize = 1 << 21;

impl Hist {
	fn new() -> Hist {
		Hist { small: vec![0; SMALL], large: Vec::new(), n: 0, sum: 0 }
	}

	#[inline]
	fn add(&mut self, v: u32) {
		self.n += 1;
		self.sum += v as u128;
		if (v as usize) < SMALL {
			self.small[v as usize] += 1;
		} else {
			self.large.push(v);
		}
	}

	fn pct(&mut self, q: f64) -> u64 {
		if self.n == 0 {
			return 0;
		}
		let target = ((self.n as f64) * q / 100.0) as u64;
		let mut seen = 0u64;
		for (v, &c) in self.small.iter().enumerate() {
			seen += c;
			if seen > target {
				return v as u64;
			}
		}
		self.large.sort_unstable();
		let idx = (target - seen) as usize;
		self.large.get(idx).copied().unwrap_or(0) as u64
	}

	fn mean(&self) -> f64 {
		match self.n {
			0 => 0.0,
			n => self.sum as f64 / n as f64,
		}
	}
}

fn commas(n: u64) -> String {
	let s = n.to_string();
	let mut o = String::new();
	for (i, c) in s.chars().enumerate() {
		if i > 0 && (s.len() - i) % 3 == 0 {
			o.push(',');
		}
		o.push(c);
	}
	o
}

const QS: [f64; 8] = [1.0, 25.0, 50.0, 75.0, 90.0, 95.0, 99.0, 99.9];

fn row(label: &str, h: &mut Hist) -> String {
	let mut s = format!("| {label} ");
	for q in QS {
		s.push_str(&format!("| {} ", commas(h.pct(q))));
	}
	s.push('|');
	s
}

fn main() {
	let path = std::env::args().nth(1).expect("usage: tracestat_full <trace.bin>");
	let name = std::path::Path::new(&path)
		.file_stem()
		.map(|s| s.to_string_lossy().to_string())
		.unwrap_or_default();

	let f = File::open(&path).expect("open");
	let mut r = BufReader::with_capacity(1 << 22, f);
	let mut buf = vec![0u8; 25 * 40_000];

	let mut t = Table::with_capacity(1 << 22);
	let (mut records, mut gets, mut sets, mut zero_size, mut zero_ttl) = (0u64, 0u64, 0u64, 0u64, 0u64);
	let mut size_by_access = Hist::new();
	let mut ttl_by_access = Hist::new();
	let mut wss: u128 = 0;

	loop {
		let mut filled = 0usize;
		while filled < buf.len() {
			match r.read(&mut buf[filled..]) {
				Ok(0) => break,
				Ok(n) => filled += n,
				Err(e) => panic!("read failed: {e}"),
			}
		}
		let recs = filled / 25;
		if recs == 0 {
			break;
		}
		for i in 0..recs {
			let b = &buf[i * 25..i * 25 + 25];
			let cmd = b[8];
			let key = u64::from_le_bytes(b[9..17].try_into().unwrap());
			let vsz = u32::from_le_bytes(b[17..21].try_into().unwrap());
			let ttl = u32::from_le_bytes(b[21..25].try_into().unwrap());

			records += 1;
			if cmd == 0 { gets += 1 } else { sets += 1 }
			if ttl == 0 { zero_ttl += 1 } else { ttl_by_access.add(ttl) }
			if vsz == 0 {
				zero_size += 1;
				continue;
			}
			size_by_access.add(vsz);
			if t.observe(key, vsz) {
				wss += vsz as u128;
			}
		}
		if filled < buf.len() {
			break;
		}
	}

	// Per-object distributions, over the resident (first-observed) sizes.
	let mut size_by_object = Hist::new();
	let mut accesses = Hist::new();
	let (mut one_hit, mut wss_max) = (0u64, 0u128);
	for s in &t.slots {
		if s.first != 0 {
			size_by_object.add(s.first);
			accesses.add(s.count);
			wss_max += s.max as u128;
			if s.count == 1 {
				one_hit += 1;
			}
		}
	}

	let distinct = t.len as u64;
	let floor = distinct as f64 / records.max(1) as f64;
	let gib = |b: u128| b as f64 / (1024.0 * 1024.0 * 1024.0);

	println!("## {name}\n");
	println!("| | |");
	println!("|---|--:|");
	println!("| replayed records (all GET) | {} |", commas(records));
	println!("| distinct objects | {} |", commas(distinct));
	println!("| **read-through WSS** | **{:.1} GiB** |", gib(wss));
	println!("| WSS if max size were used | {:.1} GiB (+{:.3}%) |", gib(wss_max),
		100.0 * (wss_max as f64 / wss.max(1) as f64 - 1.0));
	println!("| mean object | {:.0} B |", wss as f64 / distinct.max(1) as f64);
	println!("| **compulsory miss (floor)** | **{floor:.4}** |");
	println!("| **read-through get:set (inf cache)** | **{:.3}:{:.3}** |", 1.0 - floor, floor);
	println!("| accesses per object (mean) | {:.2} |", accesses.mean());
	println!("| one-hit wonders | {} ({:.1}%) |", commas(one_hit),
		100.0 * one_hit as f64 / distinct.max(1) as f64);
	println!("| records with no size | {} ({:.4}%) |", commas(zero_size),
		100.0 * zero_size as f64 / records.max(1) as f64);
	println!("| records with no TTL | {} ({:.4}%) |", commas(zero_ttl),
		100.0 * zero_ttl as f64 / records.max(1) as f64);
	println!("| non-GET records | {} |", commas(sets));
	println!();
	println!("| distribution | p1 | p25 | p50 | p75 | p90 | p95 | p99 | p99.9 |");
	println!("|---|--:|--:|--:|--:|--:|--:|--:|--:|");
	println!("{}", row("object size (per object, B)", &mut size_by_object));
	println!("{}", row("object size (per access, B)", &mut size_by_access));
	println!("{}", row("accesses per object", &mut accesses));
	println!("{}", row("TTL (per access, s)", &mut ttl_by_access));
	println!();
	println!("mean object size per access: {:.0} B; mean TTL: {:.0} s",
		size_by_access.mean(), ttl_by_access.mean());
	println!();
}
