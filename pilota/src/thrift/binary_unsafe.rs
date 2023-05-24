use std::{convert::TryInto, ptr, slice, str};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use faststr::FastStr;
use linkedbytes::LinkedBytes;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::{
    error::ProtocolErrorKind, new_protocol_error, DecodeError, DecodeErrorKind, EncodeError,
    ProtocolError, TAsyncInputProtocol, TFieldIdentifier, TInputProtocol, TLengthProtocol,
    TListIdentifier, TMapIdentifier, TMessageIdentifier, TMessageType, TOutputProtocol,
    TSetIdentifier, TStructIdentifier, TType, ZERO_COPY_THRESHOLD,
};

static VERSION_1: u32 = 0x80010000;
static VERSION_MASK: u32 = 0xffff0000;

pub struct TBinaryProtocol<T> {
    pub(crate) trans: T,
    pub(crate) buf: &'static mut [u8],
    pub(crate) index: usize,

    zero_copy: bool,
    zero_copy_len: usize,
}

impl<T> TBinaryProtocol<T> {
    /// `zero_copy` only takes effect when `T` is [`BytesMut`] for input and
    /// [`LinkedBytes`] for output.
    ///
    /// # Safety
    ///
    /// The 'buf' MUST point to the same area of trans, this is a
    /// self-referencial struct.
    ///
    /// The 'trans' MUST have enough capacity to read from or write to.
    #[inline]
    pub unsafe fn new(trans: T, buf: &'static mut [u8], zero_copy: bool) -> Self {
        Self {
            trans,
            buf,
            index: 0,
            zero_copy,
            zero_copy_len: 0,
        }
    }
}

#[inline]
fn field_type_from_u8(ttype: u8) -> Result<TType, ProtocolError> {
    let ttype: TType = ttype.try_into().map_err(|_| {
        new_protocol_error(
            ProtocolErrorKind::InvalidData,
            format!("invalid ttype {}", ttype),
        )
    })?;

    Ok(ttype)
}

impl<T> TLengthProtocol for TBinaryProtocol<T> {
    #[inline]
    fn write_message_begin_len(&mut self, identifier: &TMessageIdentifier) -> usize {
        self.write_i32_len(0) + self.write_faststr_len(&identifier.name) + self.write_i32_len(0)
    }

    #[inline]
    fn write_message_end_len(&mut self) -> usize {
        0
    }

    #[inline]
    fn write_struct_begin_len(&mut self, _identifier: &TStructIdentifier) -> usize {
        0
    }

    #[inline]
    fn write_struct_end_len(&mut self) -> usize {
        0
    }

    #[inline]
    fn write_field_begin_len(&mut self, _field_type: TType, _id: Option<i16>) -> usize {
        self.write_byte_len(0) + self.write_i16_len(0)
    }

    #[inline]
    fn write_field_end_len(&mut self) -> usize {
        0
    }

    #[inline]
    fn write_field_stop_len(&mut self) -> usize {
        self.write_byte_len(0)
    }

    #[inline]
    fn write_bool_len(&mut self, _b: bool) -> usize {
        self.write_i8_len(0)
    }

    #[inline]
    fn write_bytes_len(&mut self, b: &[u8]) -> usize {
        if self.zero_copy && b.len() >= ZERO_COPY_THRESHOLD {
            self.zero_copy_len += b.len();
        }
        self.write_i32_len(0) + b.len()
    }

    #[inline]
    fn write_byte_len(&mut self, _b: u8) -> usize {
        1
    }

    #[inline]
    fn write_uuid_len(&mut self, _u: [u8; 16]) -> usize {
        16
    }

    #[inline]
    fn write_i8_len(&mut self, _i: i8) -> usize {
        1
    }

    #[inline]
    fn write_i16_len(&mut self, _i: i16) -> usize {
        2
    }

    #[inline]
    fn write_i32_len(&mut self, _i: i32) -> usize {
        4
    }

    #[inline]
    fn write_i64_len(&mut self, _i: i64) -> usize {
        8
    }

    #[inline]
    fn write_double_len(&mut self, _d: f64) -> usize {
        8
    }

    fn write_string_len(&mut self, s: &str) -> usize {
        self.write_i32_len(0) + s.len()
    }

    #[inline]
    fn write_faststr_len(&mut self, s: &FastStr) -> usize {
        if self.zero_copy && s.len() >= ZERO_COPY_THRESHOLD {
            self.zero_copy_len += s.len();
        }
        self.write_i32_len(0) + s.len()
    }

    #[inline]
    fn write_list_begin_len(&mut self, _identifier: TListIdentifier) -> usize {
        self.write_byte_len(0) + self.write_i32_len(0)
    }

    #[inline]
    fn write_list_end_len(&mut self) -> usize {
        0
    }

    #[inline]
    fn write_set_begin_len(&mut self, _identifier: TSetIdentifier) -> usize {
        self.write_byte_len(0) + self.write_i32_len(0)
    }

    #[inline]
    fn write_set_end_len(&mut self) -> usize {
        0
    }

    #[inline]
    fn write_map_begin_len(&mut self, _identifier: TMapIdentifier) -> usize {
        self.write_byte_len(0) + self.write_byte_len(0) + self.write_i32_len(0)
    }

    #[inline]
    fn write_map_end_len(&mut self) -> usize {
        0
    }

    #[inline]
    fn write_bytes_vec_len(&mut self, b: &[u8]) -> usize {
        self.write_i32_len(0) + b.len()
    }

    #[inline]
    fn zero_copy_len(&mut self) -> usize {
        self.zero_copy_len
    }

    #[inline]
    fn reset(&mut self) {
        self.zero_copy_len = 0;
    }
}

impl TOutputProtocol for TBinaryProtocol<&mut BytesMut> {
    type BufMut = BytesMut;

    #[inline]
    fn write_message_begin(&mut self, identifier: &TMessageIdentifier) -> Result<(), EncodeError> {
        let msg_type_u8: u8 = identifier.message_type.into();
        let version = (VERSION_1 | msg_type_u8 as u32) as i32;
        self.write_i32(version)?;
        self.write_faststr(identifier.name.clone())?;
        self.write_i32(identifier.sequence_number)?;
        Ok(())
    }

    #[inline]
    fn write_message_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_struct_begin(&mut self, _: &TStructIdentifier) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_struct_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_field_begin(&mut self, field_type: TType, id: i16) -> Result<(), EncodeError> {
        unsafe {
            *self.buf.get_unchecked_mut(self.index) = field_type as u8;
            let buf: &mut [u8; 2] = self
                .buf
                .get_unchecked_mut(self.index + 1..self.index + 3)
                .try_into()
                .unwrap_unchecked();
            *buf = id.to_be_bytes();
            self.index += 3;
        }
        Ok(())
    }

    #[inline]
    fn write_field_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_field_stop(&mut self) -> Result<(), EncodeError> {
        self.write_byte(TType::Stop as u8)
    }

    #[inline]
    fn write_bool(&mut self, b: bool) -> Result<(), EncodeError> {
        if b {
            self.write_i8(1)
        } else {
            self.write_i8(0)
        }
    }

    #[inline]
    fn write_bytes(&mut self, b: Bytes) -> Result<(), EncodeError> {
        self.write_i32(b.len() as i32)?;
        unsafe {
            ptr::copy_nonoverlapping(
                b.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                b.len(),
            );
            self.index += b.len();
        }
        Ok(())
    }

    #[inline]
    fn write_byte(&mut self, b: u8) -> Result<(), EncodeError> {
        unsafe {
            *self.buf.get_unchecked_mut(self.index) = b;
            self.index += 1;
        }
        Ok(())
    }

    #[inline]
    fn write_uuid(&mut self, u: [u8; 16]) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 16] = self
                .buf
                .get_unchecked_mut(self.index..self.index + 16)
                .try_into()
                .unwrap_unchecked();
            *buf = u;
            self.index += 16;
        }
        Ok(())
    }

    #[inline]
    fn write_i8(&mut self, i: i8) -> Result<(), EncodeError> {
        unsafe {
            *self.buf.get_unchecked_mut(self.index) = *i.to_be_bytes().get_unchecked(0);
            self.index += 1;
        }
        Ok(())
    }

    #[inline]
    fn write_i16(&mut self, i: i16) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 2] = self
                .trans
                .get_unchecked_mut(self.index..self.index + 2)
                .try_into()
                .unwrap_unchecked();
            *buf = i.to_be_bytes();
            self.index += 2;
        }
        Ok(())
    }

    #[inline]
    fn write_i32(&mut self, i: i32) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 4] = self
                .trans
                .get_unchecked_mut(self.index..self.index + 4)
                .try_into()
                .unwrap_unchecked();
            *buf = i.to_be_bytes();
            self.index += 4;
        }
        Ok(())
    }

    #[inline]
    fn write_i64(&mut self, i: i64) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 8] = self
                .trans
                .get_unchecked_mut(self.index..self.index + 8)
                .try_into()
                .unwrap_unchecked();
            *buf = i.to_be_bytes();
            self.index += 8;
        }
        Ok(())
    }

    #[inline]
    fn write_double(&mut self, d: f64) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 8] = self
                .trans
                .get_unchecked_mut(self.index..self.index + 8)
                .try_into()
                .unwrap_unchecked();
            *buf = d.to_bits().to_be_bytes();
            self.index += 8;
        }
        Ok(())
    }

    #[inline]
    fn write_string(&mut self, s: &str) -> Result<(), EncodeError> {
        self.write_i32(s.len() as i32)?;
        unsafe {
            ptr::copy_nonoverlapping(
                s.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                s.len(),
            );
            self.index += s.len();
        }
        Ok(())
    }

    #[inline]
    fn write_faststr(&mut self, s: FastStr) -> Result<(), EncodeError> {
        self.write_i32(s.len() as i32)?;
        unsafe {
            ptr::copy_nonoverlapping(
                s.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                s.len(),
            );
            self.index += s.len();
        }
        Ok(())
    }

    #[inline]
    fn write_list_begin(&mut self, identifier: TListIdentifier) -> Result<(), EncodeError> {
        self.write_byte(identifier.element_type.into())?;
        self.write_i32(identifier.size as i32)
    }

    #[inline]
    fn write_list_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_set_begin(&mut self, identifier: TSetIdentifier) -> Result<(), EncodeError> {
        self.write_byte(identifier.element_type.into())?;
        self.write_i32(identifier.size as i32)
    }

    #[inline]
    fn write_set_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_map_begin(&mut self, identifier: TMapIdentifier) -> Result<(), EncodeError> {
        let key_type = identifier.key_type;
        self.write_byte(key_type.into())?;
        let val_type = identifier.value_type;
        self.write_byte(val_type.into())?;
        self.write_i32(identifier.size as i32)
    }

    #[inline]
    fn write_map_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn flush(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_bytes_vec(&mut self, b: &[u8]) -> Result<(), EncodeError> {
        self.write_i32(b.len() as i32)?;
        unsafe {
            ptr::copy_nonoverlapping(
                b.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                b.len(),
            );
            self.index += b.len();
        }
        Ok(())
    }

    #[inline]
    fn buf_mut(&mut self) -> &mut Self::BufMut {
        unimplemented!("unsafe protocol doesn't support using buf_mut")
    }
}

impl TOutputProtocol for TBinaryProtocol<&mut LinkedBytes> {
    type BufMut = LinkedBytes;

    #[inline]
    fn write_message_begin(&mut self, identifier: &TMessageIdentifier) -> Result<(), EncodeError> {
        let msg_type_u8: u8 = identifier.message_type.into();
        let version = (VERSION_1 | msg_type_u8 as u32) as i32;
        self.write_i32(version)?;
        self.write_faststr(identifier.name.clone())?;
        self.write_i32(identifier.sequence_number)?;
        Ok(())
    }

    #[inline]
    fn write_message_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_struct_begin(&mut self, _: &TStructIdentifier) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_struct_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_field_begin(&mut self, field_type: TType, id: i16) -> Result<(), EncodeError> {
        unsafe {
            *self.buf.get_unchecked_mut(self.index) = field_type as u8;
            let buf: &mut [u8; 2] = self
                .buf
                .get_unchecked_mut(self.index + 1..self.index + 3)
                .try_into()
                .unwrap_unchecked();
            *buf = id.to_be_bytes();
            self.index += 3;
        }
        Ok(())
    }

    #[inline]
    fn write_field_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_field_stop(&mut self) -> Result<(), EncodeError> {
        self.write_byte(TType::Stop as u8)
    }

    #[inline]
    fn write_bool(&mut self, b: bool) -> Result<(), EncodeError> {
        if b {
            self.write_i8(1)
        } else {
            self.write_i8(0)
        }
    }

    #[inline]
    fn write_bytes(&mut self, b: Bytes) -> Result<(), EncodeError> {
        self.write_i32(b.len() as i32)?;
        if self.zero_copy && b.len() >= ZERO_COPY_THRESHOLD {
            unsafe {
                self.trans.bytes_mut().advance_mut(self.index);
                self.index = 0;
            }
            self.trans.insert(b);
            self.buf = unsafe {
                slice::from_raw_parts_mut(
                    self.trans.bytes_mut().as_mut_ptr(),
                    self.trans.bytes_mut().len(),
                )
            };
            return Ok(());
        }
        unsafe {
            ptr::copy_nonoverlapping(
                b.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                b.len(),
            );
            self.index += b.len();
        }
        Ok(())
    }

    #[inline]
    fn write_byte(&mut self, b: u8) -> Result<(), EncodeError> {
        unsafe {
            *self.buf.get_unchecked_mut(self.index) = b;
            self.index += 1;
        }
        Ok(())
    }

    #[inline]
    fn write_uuid(&mut self, u: [u8; 16]) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 16] = self
                .trans
                .bytes_mut()
                .get_unchecked_mut(self.index..self.index + 16)
                .try_into()
                .unwrap_unchecked();
            *buf = u;
            self.index += 16;
        }
        Ok(())
    }

    #[inline]
    fn write_i8(&mut self, i: i8) -> Result<(), EncodeError> {
        unsafe {
            *self.buf.get_unchecked_mut(self.index) = *i.to_be_bytes().get_unchecked(0);
            self.index += 1;
        }
        Ok(())
    }

    #[inline]
    fn write_i16(&mut self, i: i16) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 2] = self
                .trans
                .bytes_mut()
                .get_unchecked_mut(self.index..self.index + 2)
                .try_into()
                .unwrap_unchecked();
            *buf = i.to_be_bytes();
            self.index += 2;
        }
        Ok(())
    }

    #[inline]
    fn write_i32(&mut self, i: i32) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 4] = self
                .trans
                .bytes_mut()
                .get_unchecked_mut(self.index..self.index + 4)
                .try_into()
                .unwrap_unchecked();
            *buf = i.to_be_bytes();
            self.index += 4;
        }
        Ok(())
    }

    #[inline]
    fn write_i64(&mut self, i: i64) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 8] = self
                .trans
                .bytes_mut()
                .get_unchecked_mut(self.index..self.index + 8)
                .try_into()
                .unwrap_unchecked();
            *buf = i.to_be_bytes();
            self.index += 8;
        }
        Ok(())
    }

    #[inline]
    fn write_double(&mut self, d: f64) -> Result<(), EncodeError> {
        unsafe {
            let buf: &mut [u8; 8] = self
                .trans
                .bytes_mut()
                .get_unchecked_mut(self.index..self.index + 8)
                .try_into()
                .unwrap_unchecked();
            *buf = d.to_bits().to_be_bytes();
            self.index += 8;
        }
        Ok(())
    }

    #[inline]
    fn write_string(&mut self, s: &str) -> Result<(), EncodeError> {
        self.write_i32(s.len() as i32)?;
        unsafe {
            ptr::copy_nonoverlapping(
                s.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                s.len(),
            );
            self.index += s.len();
        }
        Ok(())
    }

    #[inline]
    fn write_faststr(&mut self, s: FastStr) -> Result<(), EncodeError> {
        self.write_i32(s.len() as i32)?;
        if self.zero_copy && s.len() >= ZERO_COPY_THRESHOLD {
            unsafe {
                self.trans.bytes_mut().advance_mut(self.index);
                self.index = 0;
            }
            self.trans.insert_faststr(s);
            self.buf = unsafe {
                slice::from_raw_parts_mut(
                    self.trans.bytes_mut().as_mut_ptr(),
                    self.trans.bytes_mut().len(),
                )
            };
            return Ok(());
        }
        unsafe {
            ptr::copy_nonoverlapping(
                s.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                s.len(),
            );
            self.index += s.len();
        }
        Ok(())
    }

    #[inline]
    fn write_list_begin(&mut self, identifier: TListIdentifier) -> Result<(), EncodeError> {
        self.write_byte(identifier.element_type.into())?;
        self.write_i32(identifier.size as i32)
    }

    #[inline]
    fn write_list_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_set_begin(&mut self, identifier: TSetIdentifier) -> Result<(), EncodeError> {
        self.write_byte(identifier.element_type.into())?;
        self.write_i32(identifier.size as i32)
    }

    #[inline]
    fn write_set_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_map_begin(&mut self, identifier: TMapIdentifier) -> Result<(), EncodeError> {
        let key_type = identifier.key_type;
        self.write_byte(key_type.into())?;
        let val_type = identifier.value_type;
        self.write_byte(val_type.into())?;
        self.write_i32(identifier.size as i32)
    }

    #[inline]
    fn write_map_end(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn flush(&mut self) -> Result<(), EncodeError> {
        Ok(())
    }

    #[inline]
    fn write_bytes_vec(&mut self, b: &[u8]) -> Result<(), EncodeError> {
        self.write_i32(b.len() as i32)?;
        unsafe {
            ptr::copy_nonoverlapping(
                b.as_ptr(),
                self.buf.as_mut_ptr().offset(self.index as isize),
                b.len(),
            );
            self.index += b.len();
        }
        Ok(())
    }

    #[inline]
    fn buf_mut(&mut self) -> &mut Self::BufMut {
        unimplemented!("unsafe protocol doesn't support using buf_mut")
    }
}

pub struct TAsyncBinaryProtocol<R> {
    reader: R,
}

#[async_trait::async_trait]
impl<R> TAsyncInputProtocol for TAsyncBinaryProtocol<R>
where
    R: AsyncRead + Unpin + Send,
{
    // https://github.com/apache/thrift/blob/master/doc/specs/thrift-binary-protocol.md
    async fn read_message_begin(&mut self) -> Result<TMessageIdentifier, DecodeError> {
        let size = self.reader.read_i32().await?;
        if size > 0 {
            return Err(DecodeError::new(
                DecodeErrorKind::BadVersion,
                "Missing version in ReadMessageBegin".to_string(),
            ));
        }

        let type_u8 = (size & 0xf) as u8;

        let message_type = TMessageType::try_from(type_u8).map_err(|_| {
            DecodeError::new(
                DecodeErrorKind::InvalidData,
                format!("invalid message type {}", type_u8),
            )
        })?;

        let version = size & (VERSION_MASK as i32);
        if version != (VERSION_1 as i32) {
            return Err(DecodeError::new(
                DecodeErrorKind::BadVersion,
                "Bad version in ReadMessageBegin",
            ));
        }

        let name = self.read_faststr().await?;

        let sequence_number = self.read_i32().await?;
        Ok(TMessageIdentifier::new(name, message_type, sequence_number))
    }

    #[inline]
    async fn read_message_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    async fn read_struct_begin(&mut self) -> Result<Option<TStructIdentifier>, DecodeError> {
        Ok(None)
    }

    #[inline]
    async fn read_struct_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    async fn read_field_begin(&mut self) -> Result<TFieldIdentifier, DecodeError> {
        let field_type_byte = self.read_byte().await?;
        let field_type = field_type_byte.try_into().map_err(|_| {
            DecodeError::new(
                DecodeErrorKind::InvalidData,
                format!("invalid ttype {}", field_type_byte),
            )
        })?;
        let id = match field_type {
            TType::Stop => Ok(0),
            _ => self.read_i16().await,
        }?;
        Ok(TFieldIdentifier::new::<Option<&'static str>, i16>(
            None, field_type, id,
        ))
    }

    #[inline]
    async fn read_field_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    async fn read_bool(&mut self) -> Result<bool, DecodeError> {
        let b = self.read_i8().await?;
        match b {
            0 => Ok(false),
            _ => Ok(true),
        }
    }

    #[inline]
    async fn read_bytes(&mut self) -> Result<Bytes, DecodeError> {
        self.read_bytes_vec().await.map(Bytes::from)
    }

    #[inline]
    async fn read_bytes_vec(&mut self) -> Result<Vec<u8>, DecodeError> {
        let len = self.reader.read_i32().await? as usize;
        // FIXME: use maybe_uninit?
        let mut v = vec![0; len];
        self.reader.read_exact(&mut v).await?;
        Ok(v)
    }

    #[inline]
    async fn read_uuid(&mut self) -> Result<[u8; 16], DecodeError> {
        let mut uuid = [0; 16];
        self.reader.read_exact(&mut uuid).await?;
        Ok(uuid)
    }

    #[inline]
    async fn read_string(&mut self) -> Result<String, DecodeError> {
        let len = self.reader.read_i32().await? as usize;
        // FIXME: use maybe_uninit?
        let mut v = vec![0; len];
        self.reader.read_exact(&mut v).await?;
        Ok(unsafe { String::from_utf8_unchecked(v) })
    }

    #[inline]
    async fn read_faststr(&mut self) -> Result<FastStr, DecodeError> {
        self.read_string().await.map(FastStr::from_string)
    }

    #[inline]
    async fn read_byte(&mut self) -> Result<u8, DecodeError> {
        Ok(self.reader.read_u8().await?)
    }

    #[inline]
    async fn read_i8(&mut self) -> Result<i8, DecodeError> {
        Ok(self.reader.read_i8().await?)
    }

    #[inline]
    async fn read_i16(&mut self) -> Result<i16, DecodeError> {
        Ok(self.reader.read_i16().await?)
    }

    #[inline]
    async fn read_i32(&mut self) -> Result<i32, DecodeError> {
        Ok(self.reader.read_i32().await?)
    }

    #[inline]
    async fn read_i64(&mut self) -> Result<i64, DecodeError> {
        Ok(self.reader.read_i64().await?)
    }

    #[inline]
    async fn read_double(&mut self) -> Result<f64, DecodeError> {
        Ok(self.reader.read_f64().await?)
    }

    #[inline]
    async fn read_list_begin(&mut self) -> Result<TListIdentifier, DecodeError> {
        let element_type: TType = self
            .read_byte()
            .await
            .and_then(|n| Ok(field_type_from_u8(n)?))?;
        let size = self.read_i32().await?;
        Ok(TListIdentifier::new(element_type, size as usize))
    }

    #[inline]
    async fn read_list_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    async fn read_set_begin(&mut self) -> Result<TSetIdentifier, DecodeError> {
        let element_type: TType = self
            .read_byte()
            .await
            .and_then(|n| Ok(field_type_from_u8(n)?))?;
        let size = self.read_i32().await?;
        Ok(TSetIdentifier::new(element_type, size as usize))
    }

    #[inline]
    async fn read_set_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    async fn read_map_begin(&mut self) -> Result<TMapIdentifier, DecodeError> {
        let key_type: TType = self
            .read_byte()
            .await
            .and_then(|n| Ok(field_type_from_u8(n)?))?;
        let value_type: TType = self
            .read_byte()
            .await
            .and_then(|n| Ok(field_type_from_u8(n)?))?;
        let size = self.read_i32().await?;
        Ok(TMapIdentifier::new(key_type, value_type, size as usize))
    }

    #[inline]
    async fn read_map_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }
}

impl<R> TAsyncBinaryProtocol<R>
where
    R: AsyncRead + Unpin + Send,
{
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl TInputProtocol for TBinaryProtocol<&mut BytesMut> {
    type Buf = BytesMut;

    fn read_message_begin(&mut self) -> Result<TMessageIdentifier, DecodeError> {
        let size = self.read_i32()?;

        if size > 0 {
            return Err(DecodeError::new(
                DecodeErrorKind::BadVersion,
                "Missing version in ReadMessageBegin".to_string(),
            ));
        }
        let type_u8 = (size & 0xf) as u8;

        let message_type = TMessageType::try_from(type_u8).map_err(|_| {
            DecodeError::new(
                DecodeErrorKind::InvalidData,
                format!("invalid message type {}", type_u8),
            )
        })?;

        let version = size & (VERSION_MASK as i32);
        if version != (VERSION_1 as i32) {
            return Err(DecodeError::new(
                DecodeErrorKind::BadVersion,
                "Bad version in ReadMessageBegin",
            ));
        }

        let name = self.read_faststr()?;

        let sequence_number = self.read_i32()?;
        Ok(TMessageIdentifier::new(name, message_type, sequence_number))
    }

    #[inline]
    fn read_message_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    fn read_struct_begin(&mut self) -> Result<Option<TStructIdentifier>, DecodeError> {
        Ok(None)
    }

    #[inline]
    fn read_struct_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    fn read_field_begin(&mut self) -> Result<TFieldIdentifier, DecodeError> {
        let field_type_byte = self.read_byte()?;
        let field_type = field_type_byte.try_into().map_err(|_| {
            DecodeError::new(
                DecodeErrorKind::InvalidData,
                format!("invalid ttype {}", field_type_byte),
            )
        })?;
        let id = match field_type {
            TType::Stop => Ok(0),
            _ => self.read_i16(),
        }?;
        Ok(TFieldIdentifier::new::<Option<&'static str>, i16>(
            None, field_type, id,
        ))
    }

    #[inline]
    fn read_field_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    fn read_bool(&mut self) -> Result<bool, DecodeError> {
        let b = self.read_i8()?;
        match b {
            0 => Ok(false),
            _ => Ok(true),
        }
    }

    #[inline]
    fn read_bytes(&mut self) -> Result<Bytes, DecodeError> {
        let len = self.read_i32()?;
        self.trans.advance(self.index);
        self.index = 0;
        // split and freeze it
        let val = self.trans.split_to(len as usize).freeze();
        self.buf = unsafe { slice::from_raw_parts_mut(self.trans.as_mut_ptr(), self.trans.len()) };
        Ok(val)
    }

    #[inline]
    fn read_uuid(&mut self) -> Result<[u8; 16], DecodeError> {
        let u;
        unsafe {
            u = self
                .trans
                .get_unchecked_mut(self.index..self.index + 16)
                .try_into()
                .unwrap_unchecked();
            self.index += 16;
        }
        Ok(u)
    }

    #[inline]
    fn read_i8(&mut self) -> Result<i8, DecodeError> {
        unsafe {
            let val = *self.buf.get_unchecked(self.index) as i8;
            self.index += 1;
            Ok(val)
        }
    }

    #[inline]
    fn read_i16(&mut self) -> Result<i16, DecodeError> {
        unsafe {
            let val = self.buf.get_unchecked(self.index..self.index + 2);
            self.index += 2;
            Ok(i16::from_be_bytes(val.try_into().unwrap_unchecked()))
        }
    }

    #[inline]
    fn read_i32(&mut self) -> Result<i32, DecodeError> {
        unsafe {
            let val = self.buf.get_unchecked(self.index..self.index + 4);
            self.index += 4;
            Ok(i32::from_be_bytes(val.try_into().unwrap_unchecked()))
        }
    }

    #[inline]
    fn read_i64(&mut self) -> Result<i64, DecodeError> {
        unsafe {
            let val = self.buf.get_unchecked(self.index..self.index + 8);
            self.index += 8;
            Ok(i64::from_be_bytes(val.try_into().unwrap_unchecked()))
        }
    }

    #[inline]
    fn read_double(&mut self) -> Result<f64, DecodeError> {
        unsafe {
            let val = self.buf.get_unchecked(self.index..self.index + 8);
            self.index += 8;
            Ok(f64::from_bits(u64::from_be_bytes(
                val.try_into().unwrap_unchecked(),
            )))
        }
    }

    #[inline]
    fn read_string(&mut self) -> Result<String, DecodeError> {
        unsafe {
            let len = self.read_i32().unwrap_unchecked();
            let val = str::from_utf8_unchecked(
                self.buf
                    .get_unchecked(self.index..self.index + len as usize),
            )
            .to_string();
            self.index += len as usize;
            Ok(val)
        }
    }

    #[inline]
    fn read_faststr(&mut self) -> Result<FastStr, DecodeError> {
        unsafe {
            let len = self.read_i32().unwrap_unchecked() as usize;
            if len >= ZERO_COPY_THRESHOLD {
                self.trans.advance(self.index);
                self.index = 0;
                let bytes = self.trans.split_to(len).freeze();
                self.buf = slice::from_raw_parts_mut(self.trans.as_mut_ptr(), self.trans.len());
                return Ok(FastStr::from_bytes_unchecked(bytes));
            }

            let val = FastStr::new(str::from_utf8_unchecked(
                self.buf.get_unchecked(self.index..self.index + len),
            ));

            self.index += len;

            Ok(val)
        }
    }

    #[inline]
    fn read_list_begin(&mut self) -> Result<TListIdentifier, DecodeError> {
        let element_type: TType = self.read_byte().and_then(|n| Ok(field_type_from_u8(n)?))?;
        let size = self.read_i32()?;
        Ok(TListIdentifier::new(element_type, size as usize))
    }

    #[inline]
    fn read_list_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    fn read_set_begin(&mut self) -> Result<TSetIdentifier, DecodeError> {
        let element_type: TType = self.read_byte().and_then(|n| Ok(field_type_from_u8(n)?))?;
        let size = self.read_i32()?;
        Ok(TSetIdentifier::new(element_type, size as usize))
    }

    #[inline]
    fn read_set_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    fn read_map_begin(&mut self) -> Result<TMapIdentifier, DecodeError> {
        let key_type: TType = self.read_byte().and_then(|n| Ok(field_type_from_u8(n)?))?;
        let value_type: TType = self.read_byte().and_then(|n| Ok(field_type_from_u8(n)?))?;
        let size = self.read_i32()?;
        Ok(TMapIdentifier::new(key_type, value_type, size as usize))
    }

    #[inline]
    fn read_map_end(&mut self) -> Result<(), DecodeError> {
        Ok(())
    }

    #[inline]
    fn read_byte(&mut self) -> Result<u8, DecodeError> {
        unsafe {
            let val = *self.buf.get_unchecked(self.index);
            self.index += 1;
            Ok(val)
        }
    }

    #[inline]
    fn read_bytes_vec(&mut self) -> Result<Vec<u8>, DecodeError> {
        let len = self.read_i32()? as usize;
        self.trans.advance(self.index);
        self.index = 0;
        let val = self.trans.split_to(len).into();
        self.buf = unsafe { slice::from_raw_parts_mut(self.trans.as_mut_ptr(), self.trans.len()) };
        Ok(val)
    }

    #[inline]
    fn buf_mut(&mut self) -> &mut Self::Buf {
        unimplemented!("unsafe protocol doesn't support using buf_mut")
    }
}