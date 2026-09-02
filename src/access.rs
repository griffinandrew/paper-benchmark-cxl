/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::io::{self, Cursor};
use byteorder::{LittleEndian, ReadBytesExt};

use kwik::file::binary::{
	SizedChunk,
	ReadChunk,
	WriteChunk,
};

#[derive(PartialEq)]
pub enum Command {
	Get,
	Set,
}

pub struct Access {
	pub timestamp: u64,
	pub command: Command,

	pub key: u64,
	pub value: Box<[u8]>,

	pub ttl: Option<u32>,
}


use core::arch::x86_64::{_mm_clflush, _mm_sfence};

impl SizedChunk for Access {
	fn chunk_size() -> usize {
		25
	}
}




impl ReadChunk for Access {
	fn from_chunk(buf: &[u8]) -> io::Result<Self> {
		let mut rdr = Cursor::new(buf);

		let timestamp = rdr.read_u64::<LittleEndian>()?;

		let command_byte = rdr.read_u8()?;
		let command = Command::from_byte(command_byte)?;

		// The record's key IS a u64. It used to be .to_string()'d here and
		// parse::<u64>()'d back inside the timed GET path; the String's heap
		// read there was allocator-placement-sensitive and manufactured a
		// ~150-200 ns per-op difference between builds. See the findings doc.
		let key = rdr.read_u64::<LittleEndian>()?;

		let value_size = rdr.read_u32::<LittleEndian>()?;
		let value: Box<[u8]> = [0u8].repeat(value_size as usize).into();

		/*
        unsafe {
           let ptr = value.as_ptr();
            for i in (0..value.len()).step_by(64) {
                _mm_clflush(ptr.add(i) as *const u8);
            }
            _mm_sfence(); // Block until all cache lines are flushed to RAM
        }
		*/

		let ttl = match rdr.read_u32::<LittleEndian>()? {
			0 => None,
			ttl => Some(ttl),
		};

		let access = Access {
			timestamp,
			command,

			key,
			value,

			ttl,
		};

		Ok(access)
	}
}

impl WriteChunk for Access {
	fn as_chunk(&self, buf: &mut Vec<u8>) -> io::Result<()> {
		let key = self.key;

		let size = self.value.len() as u32;

		buf.extend_from_slice(&self.timestamp.to_le_bytes());
		buf.extend_from_slice(&self.command.as_byte().to_le_bytes());
		buf.extend_from_slice(&key.to_le_bytes());
		buf.extend_from_slice(&size.to_le_bytes());
		buf.extend_from_slice(&self.ttl.unwrap_or(0).to_le_bytes());

		Ok(())
	}
}

impl Command {
	fn from_byte(byte: u8) -> io::Result<Self> {
		match byte {
			0 => Ok(Command::Get),
			1 => Ok(Command::Set),

			_ => Err(io::Error::new(
				io::ErrorKind::InvalidData,
				"Invalid command byte."
			)),
		}
	}

	fn as_byte(&self) -> u8 {
		match self {
			Command::Get => 0,
			Command::Set => 1,
		}
	}
}

#[cfg(test)]
mod layout_tests {
	use super::*;

	/// Pins the on-disk record layout.
	///
	/// `tools/trace_fill` rewrites these trace files using its own hard-coded
	/// copy of this layout. It is dependency-free on purpose, so that
	/// preparing a trace never rebuilds paper-cache and can run while a sweep
	/// is timing -- which means it cannot share this code and must be kept in
	/// step by hand. If this test needs changing, `Record::decode`/`encode`
	/// and `CHUNK` in that tool must change with it, or it will silently
	/// corrupt every trace it touches.
	#[test]
	fn the_record_layout_is_pinned_at_25_bytes() {
		assert_eq!(Access::chunk_size(), 25);

		let access = Access {
			timestamp: 0x0807_0605_0403_0201,
			command: Command::Set,
			key: 0x100F_0E0D_0C0B_0A09,
			value: vec![0u8; 0x1234].into_boxed_slice(),
			ttl: Some(0x1716_1514),
		};

		let mut buf = Vec::new();
		access.as_chunk(&mut buf).unwrap();

		assert_eq!(buf.len(), 25, "record grew or shrank");
		assert_eq!(&buf[0..8], &[1, 2, 3, 4, 5, 6, 7, 8], "timestamp u64 LE");
		assert_eq!(buf[8], 1, "command byte, SET == 1");
		assert_eq!(&buf[9..17], &[9, 10, 11, 12, 13, 14, 15, 16], "key u64 LE");
		assert_eq!(&buf[17..21], &[0x34, 0x12, 0, 0], "value_size u32 LE");
		assert_eq!(&buf[21..25], &[0x14, 0x15, 0x16, 0x17], "ttl u32 LE");
	}

	#[test]
	fn a_record_survives_a_round_trip() {
		let access = Access {
			timestamp: 12345,
			command: Command::Get,
			key: 0xDEAD_BEEF_1234_5678,
			value: vec![0u8; 4096].into_boxed_slice(),
			ttl: Some(900),
		};

		let mut buf = Vec::new();
		access.as_chunk(&mut buf).unwrap();
		let back = Access::from_chunk(&buf).unwrap();

		assert_eq!(back.timestamp, access.timestamp);
		assert!(back.command == Command::Get);
		assert_eq!(back.key, access.key);
		assert_eq!(back.value.len(), access.value.len());
		assert_eq!(back.ttl, access.ttl);
	}

	/// `ttl == 0` on disk is the "no TTL" sentinel, not a zero-second TTL.
	/// Every GET record in the Twitter traces carries 0 here, which is why
	/// `tools/trace_fill` has to source a filled GET's TTL from a SET.
	#[test]
	fn a_zero_ttl_decodes_as_none() {
		let mut buf = vec![0u8; 25];
		buf[8] = 0;

		assert_eq!(Access::from_chunk(&buf).unwrap().ttl, None);

		buf[21..25].copy_from_slice(&300u32.to_le_bytes());

		assert_eq!(Access::from_chunk(&buf).unwrap().ttl, Some(300));
	}
}
