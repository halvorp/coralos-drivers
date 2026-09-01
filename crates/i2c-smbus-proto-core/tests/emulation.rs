// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for every SMBus-to-I2C emulation branch and completion decoder.
//!
//! Ported from Linux `drivers/i2c/i2c-core-smbus.c:319-521`. Original copyright holders: Frodo
//! Looijaard, Mark Studebaker, Jean Delvare, and the Linux I2C authors.

use i2c_core_msg_core::flags::{is_read, receives_length, I2C_M_STOP};
use i2c_smbus_proto_core::emulation::*;
use i2c_smbus_proto_core::pec::PecRefusal;
use i2c_smbus_proto_core::protocol::{Direction, Transaction, I2C_SMBUS_BLOCK_MAX};

fn block(length: u8) -> Data {
    let mut bytes = [0; I2C_SMBUS_BLOCK_MAX];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = 0x80 + index as u8;
    }
    Data::Block { length, bytes }
}

#[test]
fn buffer_sizes_are_linux_literals() {
    assert_eq!(WRITE_BUFFER_CAPACITY, 35); // BLOCK_MAX + 3, i2c-core-smbus.c:334
    assert_eq!(READ_BUFFER_CAPACITY, 34); // BLOCK_MAX + 2, i2c-core-smbus.c:335
}

#[test]
fn quick_uses_one_empty_message_and_direction_as_data() {
    let write = emulate(
        0x2c,
        0,
        Direction::Write,
        0xa5,
        Transaction::Quick,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(write.message_count, 1);
    assert_eq!(write.messages[0].length, 0);
    assert!(!is_read(write.messages[0].flags));
    let flagged = emulate(
        0x2c,
        I2C_M_STOP,
        Direction::Read,
        0xa5,
        Transaction::Quick,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(flagged.messages[0].flags & I2C_M_STOP, I2C_M_STOP);
    assert!(is_read(flagged.messages[0].flags));

    let read = emulate(
        0x2c,
        0,
        Direction::Read,
        0xa5,
        Transaction::Quick,
        Data::None,
        true,
    )
    .unwrap();
    assert_eq!(read.message_count, 1);
    assert_eq!(read.messages[0].length, 0);
    assert!(is_read(read.messages[0].flags));
    assert!(!read.pec, "QUICK excludes PEC at i2c-core-smbus.c:352-353");
    assert_eq!(decode_read(Transaction::Quick, &read, 0), Ok(Data::None));
}

#[test]
fn byte_write_sends_command_while_byte_read_is_read_only() {
    let write = emulate(
        0x2c,
        0,
        Direction::Write,
        0xa5,
        Transaction::Byte,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(write.message_count, 1);
    assert!(!is_read(write.messages[0].flags));
    assert_eq!(write.messages[0].length, 1);
    assert_eq!(&write.messages[0].buffer[..1], &[0xa5]);

    let mut read = emulate(
        0x2c,
        0,
        Direction::Read,
        0,
        Transaction::Byte,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(read.message_count, 1);
    assert!(is_read(read.messages[0].flags));
    read.messages[0].buffer[0] = 0x6d;
    assert_eq!(
        decode_read(Transaction::Byte, &read, 1),
        Ok(Data::Byte(0x6d))
    );
}

#[test]
fn byte_data_uses_command_data_write_or_command_then_one_byte_read() {
    let write = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::ByteData,
        Data::Byte(0xab),
        false,
    )
    .unwrap();
    assert_eq!(write.message_count, 1);
    assert_eq!(write.messages[0].length, 2);
    assert_eq!(&write.messages[0].buffer[..2], &[0x10, 0xab]);

    let mut read = emulate(
        0x2c,
        0,
        Direction::Read,
        0x10,
        Transaction::ByteData,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(read.message_count, 2);
    assert_eq!(&read.messages[0].buffer[..1], &[0x10]);
    assert_eq!(read.messages[1].length, 1);
    read.messages[1].buffer[0] = 0xcd;
    assert_eq!(
        decode_read(Transaction::ByteData, &read, 1),
        Ok(Data::Byte(0xcd))
    );
}

#[test]
fn word_data_is_little_endian_in_both_directions() {
    let write = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::WordData,
        Data::Word(0x1234),
        false,
    )
    .unwrap();
    assert_eq!(&write.messages[0].buffer[..3], &[0x10, 0x34, 0x12]); // smbus.c:383-385

    let mut read = emulate(
        0x2c,
        0,
        Direction::Read,
        0x10,
        Transaction::WordData,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(read.messages[1].length, 2);
    read.messages[1].buffer[..2].copy_from_slice(&[0x78, 0x56]);
    assert_eq!(
        decode_read(Transaction::WordData, &read, 2),
        Ok(Data::Word(0x5678))
    );
}

#[test]
fn procedure_call_always_writes_word_then_reads_word() {
    // The input direction is deliberately WRITE: Linux overrides it to READ (:388-394).
    let mut transfer = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::ProcCall,
        Data::Word(0x1234),
        false,
    )
    .unwrap();
    assert_eq!(transfer.message_count, 2);
    assert_eq!(transfer.completion_direction, Direction::Read);
    assert_eq!(&transfer.messages[0].buffer[..3], &[0x10, 0x34, 0x12]);
    assert_eq!(transfer.messages[1].length, 2);
    transfer.messages[1].buffer[..2].copy_from_slice(&[0xbc, 0x9a]);
    assert_eq!(
        decode_read(Transaction::ProcCall, &transfer, 2),
        Ok(Data::Word(0x9abc))
    );
}

#[test]
fn block_data_write_carries_command_length_then_payload() {
    let transfer = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::BlockData,
        block(3),
        false,
    )
    .unwrap();
    assert_eq!(transfer.message_count, 1);
    assert_eq!(transfer.messages[0].length, 5);
    assert_eq!(
        &transfer.messages[0].buffer[..5],
        &[0x10, 0x03, 0x80, 0x81, 0x82]
    );
}

#[test]
fn block_data_read_requests_a_device_supplied_length() {
    let transfer = emulate(
        0x2c,
        0,
        Direction::Read,
        0x10,
        Transaction::BlockData,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(transfer.message_count, 2);
    assert_eq!(transfer.messages[1].length, 1); // initial length byte, smbus.c:399-400
    assert!(receives_length(transfer.messages[1].flags)); // I2C_M_RECV_LEN, :398
}

#[test]
fn returned_block_length_boundary_is_trusted_only_after_bounds_checking() {
    let mut accepted = emulate(
        0x2c,
        0,
        Direction::Read,
        0x10,
        Transaction::BlockData,
        Data::None,
        false,
    )
    .unwrap();
    accepted.messages[1].buffer[0] = 32; // I2C_SMBUS_BLOCK_MAX, uapi i2c.h:141
    for index in 0..32 {
        accepted.messages[1].buffer[index + 1] = index as u8;
    }
    let decoded = decode_read(Transaction::BlockData, &accepted, 33).unwrap();
    match decoded {
        Data::Block { length, bytes } => {
            assert_eq!(length, 32);
            assert_eq!(bytes[0], 0);
            assert_eq!(bytes[31], 31);
        }
        other => panic!("expected 32-byte block, got {other:?}"),
    }

    let mut over = accepted;
    over.messages[1].buffer[0] = 33;
    assert_eq!(
        decode_read(Transaction::BlockData, &over, 1),
        Err(Refusal::InvalidBlockSizeReturned {
            length: 33,
            maximum: 32
        })
    ); // i2c-core-smbus.c:501-508

    let mut hostile = accepted;
    hostile.messages[1].buffer[0] = 255;
    assert_eq!(
        decode_read(Transaction::BlockData, &hostile, 1),
        Err(Refusal::InvalidBlockSizeReturned {
            length: 255,
            maximum: 32
        })
    );
}

#[test]
fn block_proc_call_always_writes_then_reads_device_sized_block() {
    let mut transfer = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::BlockProcCall,
        block(2),
        false,
    )
    .unwrap();
    assert_eq!(transfer.message_count, 2);
    assert_eq!(transfer.completion_direction, Direction::Read);
    assert_eq!(&transfer.messages[0].buffer[..4], &[0x10, 0x02, 0x80, 0x81]);
    assert!(receives_length(transfer.messages[1].flags));
    assert_eq!(transfer.messages[1].length, 1);
    transfer.messages[1].buffer[..3].copy_from_slice(&[2, 0xaa, 0xbb]);
    match decode_read(Transaction::BlockProcCall, &transfer, 3).unwrap() {
        Data::Block { length, bytes } => assert_eq!((length, &bytes[..2]), (2, &[0xaa, 0xbb][..])),
        other => panic!("expected block, got {other:?}"),
    }
}

#[test]
fn i2c_block_has_fixed_read_length_and_no_wire_length_on_write() {
    let write = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::I2cBlockData,
        block(3),
        true,
    )
    .unwrap();
    assert_eq!(write.message_count, 1);
    assert_eq!(&write.messages[0].buffer[..4], &[0x10, 0x80, 0x81, 0x82]);
    assert!(!write.pec, "I2C_BLOCK_DATA excludes PEC at smbus.c:352-353");

    let mut read = emulate(
        0x2c,
        0,
        Direction::Read,
        0x10,
        Transaction::I2cBlockData,
        block(3),
        true,
    )
    .unwrap();
    assert_eq!(read.messages[1].length, 3);
    assert!(!receives_length(read.messages[1].flags));
    read.messages[1].buffer[..3].copy_from_slice(&[0x11, 0x22, 0x33]);
    match decode_read(Transaction::I2cBlockData, &read, 3).unwrap() {
        Data::Block { length, bytes } => {
            assert_eq!((length, &bytes[..3]), (3, &[0x11, 0x22, 0x33][..]))
        }
        other => panic!("expected block, got {other:?}"),
    }
}

#[test]
fn every_oversized_outbound_block_gets_a_named_refusal() {
    assert_eq!(
        emulate(
            0x2c,
            0,
            Direction::Write,
            0,
            Transaction::BlockData,
            block(33),
            false
        ),
        Err(Refusal::InvalidBlockWriteSize {
            length: 33,
            maximum: 32
        })
    );
    assert_eq!(
        emulate(
            0x2c,
            0,
            Direction::Write,
            0,
            Transaction::BlockProcCall,
            block(33),
            false
        ),
        Err(Refusal::InvalidBlockWriteSize {
            length: 33,
            maximum: 32
        })
    );
    assert_eq!(
        emulate(
            0x2c,
            0,
            Direction::Read,
            0,
            Transaction::I2cBlockData,
            block(33),
            false
        ),
        Err(Refusal::InvalidI2cBlockSize {
            direction: Direction::Read,
            length: 33,
            maximum: 32,
        })
    );
}

#[test]
fn wrong_payload_shape_and_short_response_are_named() {
    assert_eq!(
        emulate(
            0x2c,
            0,
            Direction::Write,
            0,
            Transaction::WordData,
            Data::Byte(1),
            false
        ),
        Err(Refusal::WrongDataForTransaction {
            transaction: Transaction::WordData,
            expected: "word",
        })
    );
    let read = emulate(
        0x2c,
        0,
        Direction::Read,
        0,
        Transaction::WordData,
        Data::None,
        false,
    )
    .unwrap();
    assert_eq!(
        decode_read(Transaction::WordData, &read, 1),
        Err(Refusal::ResponseTooShort {
            length: 1,
            minimum: 2
        })
    );
}

#[test]
fn pec_is_appended_to_write_only_and_reserved_on_the_final_read() {
    let write = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::ByteData,
        Data::Byte(0xab),
        true,
    )
    .unwrap();
    assert_eq!(write.messages[0].length, 3);
    assert_eq!(&write.messages[0].buffer[..3], &[0x10, 0xab, 0x7a]);

    let mut read = emulate(
        0x2c,
        0,
        Direction::Read,
        0x10,
        Transaction::ByteData,
        Data::None,
        true,
    )
    .unwrap();
    assert_eq!(read.messages[1].length, 2); // one payload plus requested PEC, smbus.c:465-467
    assert_eq!(read.partial_pec, 0xd4); // CRC over write address 0x58 and command 0x10
    read.messages[1].buffer[..2].copy_from_slice(&[0xcd, 0x32]);
    assert_eq!(
        decode_read(Transaction::ByteData, &read, 2),
        Ok(Data::Byte(0xcd))
    );
    read.messages[1].buffer[1] = 0x33;
    assert_eq!(
        decode_read(Transaction::ByteData, &read, 2),
        Err(Refusal::Pec(PecRefusal::BadPacketErrorCode {
            received: 0x33,
            expected: 0x32,
        }))
    );
}

#[test]
fn decoding_a_write_completion_has_no_read_payload() {
    let write = emulate(
        0x2c,
        0,
        Direction::Write,
        0x10,
        Transaction::ByteData,
        Data::Byte(0xab),
        false,
    )
    .unwrap();
    assert_eq!(
        decode_read(Transaction::ByteData, &write, 0),
        Ok(Data::None)
    );
}
