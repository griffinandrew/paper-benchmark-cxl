/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::{
	io,
	ops::AddAssign,
	path::Path,
	time::{Instant, Duration},
};

use statrs::statistics::{Data, OrderStatistics};

use kwik::{
	fmt,
	math,
	table::{
		Table,
		Row,
		Align,
		Style,
	},
	plot::{
		Plot,
		Figure,
		line_plot::{LinePlot, Line},
	},
	file::{
		FileWriter,
		csv::{CsvWriter, RowData, WriteRow},
	},
	tma::TimeMovingAverage,
};

type LatencyData = Data<Vec<f64>>;

#[derive(Debug, Default, Clone)]
pub struct Stats {
	ping_latencies: Vec<(Instant, Duration)>,
	get_latencies: Vec<(Instant, Duration)>,
	set_latencies: Vec<(Instant, Duration)>,

	get_total_size: u64,
	set_total_size: u64,
}

struct PercentileLatency {
	percentile: usize,

	ping_latency: Option<f64>,
	get_latency: Option<f64>,
	set_latency: Option<f64>,
}

impl Stats {
	/// Pre-sizes `get_latencies`/`set_latencies` instead of letting them
	/// grow via `Vec::push`'s amortized doubling. Over a full trace run
	/// (millions of pushes per client), that doubling means dozens of
	/// progressively larger reallocations spread across the *entire* run,
	/// each one leaving the previous (smaller) buffer's freed space behind
	/// -- on a strictly NUMA-bound allocator (see paper-cache's
	/// `DramMultiArenaObjects`), that accumulates real fragmentation, and
	/// the *final* reallocation (the single largest one, tens of MB) lands
	/// exactly when node 0 is most fragmented: at the very end of the run.
	/// Confirmed directly: this was the actual cause of a real
	/// `memory allocation of N bytes failed` abort under `-c 8` against
	/// the full standard_web.bin trace. Pre-sizing once, up front (when
	/// node 0 is fresh), replaces "many reallocations culminating in one
	/// large one at the worst possible time" with "one allocation at the
	/// best possible time." `expected_accesses` doesn't need to be exact
	/// (the real per-client split isn't perfectly even, and GETs/SETs
	/// aren't the same count) -- it only needs to be in the right order of
	/// magnitude to eliminate most of the incremental regrowth.
	pub fn with_capacity(expected_accesses: usize) -> Self {
		Stats {
			get_latencies: Vec::with_capacity(expected_accesses),
			set_latencies: Vec::with_capacity(expected_accesses),
			..Default::default()
		}
	}

	pub fn store_ping_time(&mut self, instant: Instant) {
		self.ping_latencies.push((instant, instant.elapsed()));
	}

	pub fn store_get_time(&mut self, instant: Instant) {
		self.get_latencies.push((instant, instant.elapsed()));
	}

	pub fn store_get_size(&mut self, size: u64) {
		self.get_total_size += size;
	}

	pub fn store_set_time(&mut self, instant: Instant) {
		self.set_latencies.push((instant, instant.elapsed()));
	}

	pub fn store_set_size(&mut self, size: u64) {
		self.set_total_size += size;
	}

	pub fn print_ping_stats(&self) {
		print_stats("PING", &self.ping_latencies);
	}

	pub fn print_get_stats(&self) {
		print_stats("GET", &self.get_latencies);

		if self.get_latencies.is_empty() {
			return;
		}

		let avg_size = (self.get_total_size as f64 / self.get_latencies.len() as f64) as u64;

		println!(
			"Avg GET size:\t{} ({} B)",
			fmt::memory(avg_size, Some(2)),
			fmt::number(avg_size),
		);

		let total_time = self.get_latencies
			.iter()
			.map(|(_, duration)| duration)
			.sum::<Duration>();

		let bandwidth = self.get_total_size as f64 / total_time.as_secs_f64();

		println!(
			"Bandwidth:\t{}/s ({} B/s)",
			fmt::memory(bandwidth, Some(2)),
			fmt::number(bandwidth.round()),
		);
	}

	pub fn print_set_stats(&self) {
		print_stats("SET", &self.set_latencies);

		if self.set_latencies.is_empty() {
			return;
		}

		let avg_size = (self.set_total_size as f64 / self.set_latencies.len() as f64) as u64;

		println!(
			"Avg SET size:\t{} ({} B)",
			fmt::memory(avg_size, Some(2)),
			fmt::number(avg_size),
		);

		let total_time = self.set_latencies
			.iter()
			.map(|(_, duration)| duration)
			.sum::<Duration>();

		let bandwidth = self.set_total_size as f64 / total_time.as_secs_f64();

		println!(
			"Bandwidth:\t{}/s ({} B/s)",
			fmt::memory(bandwidth, Some(2)),
			fmt::number(bandwidth.round()),
		);
	}

	pub fn save_latency_percentiles<P>(&self, path: P) -> io::Result<()>
	where
		P: AsRef<Path>,
	{
		let mut headers: Vec<&str> = vec!["Percentile"];

		if !self.ping_latencies.is_empty() {
			headers.push("Ping");
		}

		if !self.get_latencies.is_empty() {
			headers.push("Get");
		}

		if !self.set_latencies.is_empty() {
			headers.push("Set");
		}

		let mut writer = CsvWriter::<PercentileLatency>::from_path(path)?
			.with_headers(&headers)?;

		let ping_latencies = self.ping_latencies
			.iter()
			.map(|(_, duration)| duration.as_nanos() as f64)
			.collect::<Vec<_>>();

		let get_latencies = self.get_latencies
			.iter()
			.map(|(_, duration)| duration.as_nanos() as f64)
			.collect::<Vec<_>>();

		let set_latencies = self.set_latencies
			.iter()
			.map(|(_, duration)| duration.as_nanos() as f64)
			.collect::<Vec<_>>();

		let mut ping_data = Data::new(ping_latencies);
		let mut get_data = Data::new(get_latencies);
		let mut set_data = Data::new(set_latencies);

		for percentile in 1..=100 {
			let ping_latency = if !self.ping_latencies.is_empty() {
				Some(ping_data.percentile(percentile))
			} else {
				None
			};

			let get_latency = if !self.get_latencies.is_empty() {
				Some(get_data.percentile(percentile))
			} else {
				None
			};

			let set_latency = if !self.set_latencies.is_empty() {
				Some(set_data.percentile(percentile))
			} else {
				None
			};

			let percentile_latency = PercentileLatency {
				percentile,

				ping_latency,
				get_latency,
				set_latency,
			};

			writer.write_row(&percentile_latency)?;
		}

		Ok(())
	}

	pub fn save_latency_plot<P>(&self, path: P) -> io::Result<()>
	where
		P: AsRef<Path>,
	{
		let mut plot = LinePlot::default()
			.with_title("Paper latency")
			.with_x_label("Time (s)")
			.with_y_label("Latency (us)")
			.with_x_min(0)
			.with_y_min(0);

		let mut ping_line = Line::default().with_label("Ping");
		let mut get_line = Line::default().with_label("Get");
		let mut set_line = Line::default().with_label("Set");

		if let Some((initial_instant, final_instant)) = self.get_initial_instant().zip(self.get_final_instant()) {
			plot.set_x_max(final_instant.duration_since(initial_instant).as_secs_f64());

			let mut ping_tma = TimeMovingAverage::default();
			let mut get_tma = TimeMovingAverage::default();
			let mut set_tma = TimeMovingAverage::default();

			for (instant, duration) in &self.ping_latencies {
				ping_tma.push(*instant, duration.as_nanos());
			}

			for (instant, duration) in &self.get_latencies {
				get_tma.push(*instant, duration.as_nanos());
			}

			for (instant, duration) in &self.set_latencies {
				set_tma.push(*instant, duration.as_nanos());
			}

			let window = final_instant.duration_since(initial_instant) / 50;

			for (instant, value) in ping_tma.window_iter(window) {
				ping_line.push(
					instant.duration_since(initial_instant).as_secs_f64(),
					value,
				);
			}

			for (instant, value) in get_tma.window_iter(window) {
				get_line.push(
					instant.duration_since(initial_instant).as_secs_f64(),
					value,
				);
			}

			for (instant, value) in set_tma.window_iter(window) {
				set_line.push(
					instant.duration_since(initial_instant).as_secs_f64(),
					value,
				);
			}
		}

		if !ping_line.is_empty() {
			plot.line(ping_line);
		}

		if !get_line.is_empty() {
			plot.line(get_line);
		}

		if !set_line.is_empty() {
			plot.line(set_line);
		}

		let mut figure = Figure::default();

		figure.add(plot);
		figure.save(path)
	}

	fn get_initial_instant(&self) -> Option<Instant> {
		let ping_initial_instant = self.ping_latencies.first().map(|(instant, _)| *instant);
		let get_initial_instant = self.get_latencies.first().map(|(instant, _)| *instant);
		let set_initial_instant = self.set_latencies.first().map(|(instant, _)| *instant);

		let instants = &[ping_initial_instant, get_initial_instant, set_initial_instant]
			.iter()
			.flatten()
			.copied()
			.collect::<Vec<_>>();

		math::min(instants).copied()
	}

	fn get_final_instant(&self) -> Option<Instant> {
		let ping_final_instant = self.ping_latencies.last().map(|(instant, _)| *instant);
		let get_final_instant = self.get_latencies.last().map(|(instant, _)| *instant);
		let set_final_instant = self.set_latencies.last().map(|(instant, _)| *instant);

		let instants = &[ping_final_instant, get_final_instant, set_final_instant]
			.iter()
			.flatten()
			.copied()
			.collect::<Vec<_>>();

		math::max(instants).copied()
	}
}

impl AddAssign for Stats {
	fn add_assign(&mut self, rhs: Self) {
		*self = Stats {
			ping_latencies: merge_times(&self.ping_latencies, &rhs.ping_latencies),
			get_latencies: merge_times(&self.get_latencies, &rhs.get_latencies),
			set_latencies: merge_times(&self.set_latencies, &rhs.set_latencies),

			get_total_size: self.get_total_size + rhs.get_total_size,
			set_total_size: self.set_total_size + rhs.set_total_size,
		}
	}
}

fn print_stats(label: &'static str, times: &[(Instant, Duration)]) {
	let latencies = times
		.iter()
		.map(|(_, duration)| duration.as_nanos() as f64)
		.collect::<Vec<_>>();

	let mut data = Data::new(latencies);

	if data.is_empty() {
		return;
	}

	println!("\n*** {label} stats ***\n");

	print_dist(&mut data);
	print_simple_stats(label, &data);
}

fn print_dist(data: &mut LatencyData) {
	let mut table = Table::default();

	let quantiles: &[f64] = &[
		0.5,
		0.75,
		0.90,
		0.95,
		0.99,
		0.999,
		0.9999,
		0.99999,
		1.0,
	];

	let mut header = Row::default();
	let mut row = Row::default();

	for quantile in quantiles {
		let multiplier = match quantile {
			0.999 => 1000.0,
			0.9999 => 10000.0,
			0.99999 => 100000.0,
			_ => 100.0,
		};

		let label = format!("p{}", (quantile * multiplier).round());
		let value = format!("{:.0}ns", data.quantile(*quantile));

		header = header.push(label, Align::Center, Style::Bold);
		row = row.push(value, Align::Center, Style::Normal);
	}

	table.set_header(header);
	table.add_row(row);

	let mut stdout = io::stdout().lock();
	table.print(&mut stdout);
}

fn print_simple_stats(label: &'static str, data: &LatencyData) {
	let total_time = data
		.iter()
		.sum::<f64>();

	println!(
		"\n{label} avg latency:\t{}ns",
		(total_time / data.len() as f64).round(),
	);


	// for ms
	//let rate = data.len() as f64 / (total_time / 1_000_000.0);

	// want as nanos
	let rate = data.len() as f64 / (total_time / 1_000_000_000.0);

	println!(
		"{label}s/sec:\t{}",
		fmt::number(rate as u64),
	);
}

fn merge_times(times_a: &[(Instant, Duration)], times_b: &[(Instant, Duration)]) -> Vec<(Instant, Duration)> {
	let mut times = Vec::<(Instant, Duration)>::new();

	times.extend_from_slice(times_a);
	times.extend_from_slice(times_b);

	times.sort_unstable_by_key(|(instant, _)| *instant);

	times
}


#[cfg(not(feature = "hot_fix"))] //this is wrong... but want to see if it will run ..... 
impl WriteRow for PercentileLatency {
	fn as_row(&self, row: &mut RowData) -> io::Result<()> {
		row.push(self.percentile);

		if let Some(latency) = self.ping_latency {
			row.push(latency);
		}

		if let Some(latency) = self.get_latency {
			row.push(latency);
		}

		if let Some(latency) = self.set_latency {
			row.push(latency);
		}

		Ok(())
	}
}



#[cfg(feature = "hot_fix")] //this is wrong... but want to see if it will run .....
impl WriteRow for PercentileLatency {
	fn as_row(&self, row: &mut RowData) -> () {
		row.push(self.percentile);

		if let Some(latency) = self.ping_latency {
			row.push(latency);
		}

		if let Some(latency) = self.get_latency {
			row.push(latency);
		}

		if let Some(latency) = self.set_latency {
			row.push(latency);
		}

		()
	}
}
