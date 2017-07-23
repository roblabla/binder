use std;
use std::io::{Read, Write, Seek, SeekFrom, Cursor};
use std::rc::Rc;
use std::cell::RefCell;
use std::mem::size_of;

use byteorder::{ReadBytesExt, WriteBytesExt, NativeEndian};
use encoding::codec::utf_16::UTF_16LE_ENCODING;
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use {BinderConnection, Result, BinderResult, BinderError, BinderErrorKind, Handle};
use sys::{self, flat_binder_object};

macro_rules! pad_size {
    ($s:expr) => {{
        assert!($s < (!0usize) - 3, "s is too big ({} > {})", $s, (!0usize) - 3);
        ($s + 3) & (!3)
    }}
}

pub trait Reader: Read + Seek {
    fn position(&self) -> u64;
}
impl<T: AsRef<[u8]>> Reader for Cursor<T> {
    fn position(&self) -> u64 {
        self.position()
    }
}

// TODO: Might want to put some constraint on Parcel, such as implementing Debug
pub trait Parcel {
    fn data(&mut self) -> &mut Reader;
    fn objects(&self) -> &[usize];
    fn has_data(&self) -> bool;
    fn conn_mut(&mut self) -> &mut BinderConnection;
    // TODO: Disallow usage
    fn as_data_slice_mut(&mut self) -> &mut [u8];
    fn as_objects_slice_mut(&mut self) -> &mut [usize];

    fn read_buf(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let padded = pad_size!(buf.len());
        self.data().read_exact(buf)?;
        // TODO: We should make this fail *hard* if there isn't enough data !!
        self.data().seek(SeekFrom::Current((padded - buf.len()) as i64))?;
        Ok(())
    }

    // TODO: I *might* want this to be unsafe ?
    // TODO: Problem with trait objects !!! We can't turn Parcel into a trait
    // object
    // TODO: https://github.com/rust-lang/rust/issues/43408
    /*fn read_type<T: Sized>(&mut self) -> std::io::Result<T> {
        let arr : [u8; std::mem::size_of::<T>()]= unsafe { std::mem::uninitialized() };
        self.read_buf(&mut arr)?;
        Ok(std::mem::transmute(arr))
    }*/

    // TODO: Basically the only error should be UnexpectedEOF here. Returning
    // an io::Error for this is overkill. Which sucks ! We should fix that.
    fn read_i32(&mut self) -> std::io::Result<i32> {
        self.data().read_i32::<NativeEndian>()
    }

    fn read_u32(&mut self) -> std::io::Result<u32> {
        self.data().read_u32::<NativeEndian>()
    }

    fn read_string16(&mut self) -> std::io::Result<String> {
        let char_len = self.read_i32()?;
        // TODO: Bounds checking
        /*if char_len < 0 || char_len >= i32::max_value() {
            // 
            return;
        */
        println!("Allocating vec of size {} for string16", char_len);
        let mut vec = Vec::with_capacity((char_len as usize + 1) * 2);
        unsafe {
            let capacity = vec.capacity();
            self.read_buf(vec.get_unchecked_mut(..capacity))?;
            vec.set_len(capacity);
        }
        // TODO: Remove the last two bytes if they are == 0 (null character)
        // TODO: Might want to return this ?
        Ok(UTF_16LE_ENCODING.decode(vec.as_slice(), DecoderTrap::Replace)
            .expect("Decoding from UTF16 should never fail"))
    }


    fn read_strong_binder(&mut self) -> BinderResult<Option<Rc<RefCell<Handle>>>> {
        match self.read_object(false) {
            // TODO: This is a local object (I.E. belongs to my own address-space).
            // I don't really need to support this for now, as I don't even
            // support writing raw binders.
            /*Ok(flat_binder_object { type_: sys::BinderType::Binder } @ flat) =>
                flat.cookie,*/
            // TODO: Some food for thought : I basically should never need to access
            // conn in an OwnedParcel, only in a BinderParcel. Because of this,
            // it might be worth moving read_strong_binder impl outside. Then,
            // we could go back to OwnedParcel being basically independant of a
            // BinderConnection !
            //
            // I should make sure this is true though. writeObject does some
            // stuff with ProcessState in libbinder. I might be able to side
            // step it as well, but it might not be so easy
            Ok(flat) if flat.type_ == sys::BinderType::Handle as u32 =>
                Ok(Some(self.conn_mut().get_strong_proxy_for_handle(unsafe { flat.target.handle }))),
            _ => Err(BinderErrorKind::BadType.into())
        }
    }

    // TODO: Private ?
    fn read_object(&mut self, null_metadata: bool) -> Result<flat_binder_object> {
        let mut buf = [0; size_of::<flat_binder_object>()];
        let dpos = self.data().position();
        self.read_buf(&mut buf)?;
        let obj : sys::flat_binder_object = unsafe { std::mem::transmute(buf) };
        if null_metadata && obj.cookie == 0 && unsafe { obj.target.binder == 0 } {
            // When transferring a NULL object, we don't write it into the
            // object list, so we don't want to check for it when reading.
            Ok(obj)
        } else {
            // Ensure that this object is valid
            let objs = self.objects();
            if let Ok(_) = objs.binary_search(&(dpos as usize)) {
                Ok(obj)
            } else {
                Err(BinderError::from_kind(BinderErrorKind::BadType).into())
            }
        }
    }
}

// In the libbinder parcel, mOwner is a "free" function that is provided in the
// case where the Parcel doesn't own the data buffer. When you try to grow data
// that doesn't belong to you, there's a bit of very complicated logic in there.
//
// I think it'd be a good idea to have a `ParcelBuilder` that accepts *only*
// writes, and a read-only Parcel.

/// The main type of `Parcel`. An `OwnedParcel` is (mostly) independant of the
/// underlying `BinderConnection` : it owns its underlying data buffer.
///
/// Right now it still requires access to a `BinderConnection` though (which is
/// also why `BinderConnection` is an `Rc<RefCell>`) in order to register
/// handles to the connection's Handle HashMap. It is my hope to eventually
/// break this dependency, perhaps by requiring the `BinderConnection` to be
/// supplied as an argument to the `read_*_binder` functions.
#[derive(Debug)]
pub struct OwnedParcel {
    data: Cursor<Vec<u8>>,
    // TODO: What is this even for ? It seems to point at offsets within data,
    // but it's a bit weird
    objects: Vec<usize>,
    conn: BinderConnection,
    /*/// An optimization hint when looking for an object. Unused for now.
    next_object_hint: usize,*/

    /// Tristate for whether we have fds or not. If None, we do not know (we
    /// need to scan the objects array). If Some(true), we do, if Some(false),
    /// we don't.
    has_fds: Option<bool>,
    /// Whether this Parcel allows writing fds to it.
    allow_fds: bool
}

impl OwnedParcel {
    pub fn new(conn: BinderConnection) -> OwnedParcel {
        OwnedParcel {
            data: Cursor::new(Vec::with_capacity(256)),
            objects: Vec::with_capacity(16),
            conn: conn,
            has_fds: Some(false),
            allow_fds: true
        }
    }

    // TODO: Maybe this should go in Parcel...
    pub fn len(&self) -> usize {
        self.data.get_ref().len()
    }

    // TODO: Maybe I should let others access it ?
    pub(crate) fn capacity(&self) -> usize {
        self.data.get_ref().capacity()
    }

    pub unsafe fn set_data_len(&mut self, size: usize) {
        self.data.get_mut().set_len(size)
    }

    pub fn set_position(&mut self, pos: usize) {
        self.data.set_position(pos as u64)
    }

    pub fn clear(&mut self) {
        self.data.set_position(0);
        self.data.get_mut().clear();
    }

    // TODO: I decided to make the write panic!() instead of return an error.
    // The idea is that we're going to want to blow up *anyway* one way or
    // another. If I do it at the source, we'll get a deeper backtrace (though
    // error-chain gives us one too)
    pub fn write_i32(&mut self, val: i32) {
        self.data.write_i32::<NativeEndian>(val).expect("Write bigger than usize");
    }

    pub fn write_u32(&mut self, val: u32) {
        self.data.write_u32::<NativeEndian>(val).expect("Write bigger than usize");
    }

    pub fn write_pointer(&mut self, val: sys::binder_uintptr_t) {
        let buf : [u8; size_of::<sys::binder_uintptr_t>()] = unsafe { std::mem::transmute(val) };
        self.data.write(&buf).expect("Write bigger than usize");
    }

    pub fn write_interface_token(&mut self, interface: &str) {
        // TODO: strict-mode policy
        self.write_i32(0);
        self.write_string16(interface);
    }

    pub fn write_string16(&mut self, s: &str) {
        self.write_i32(s.len() as i32);
        // TODO: Might want to return this ?
        let mut vec = UTF_16LE_ENCODING.encode(s, EncoderTrap::Replace)
            .expect("Encoding in UTF16 should never fail");
        // Add \0 as char16_t
        vec.extend([0, 0].iter());
        self.write_buf(&vec)
    }

    // TODO: This should take the eventual Binder enum
    pub fn write_strong_binder(&mut self, handle: Option<Rc<RefCell<Handle>>>) {
        let mut obj : flat_binder_object = unsafe { std::mem::zeroed() };
        // TODO: FLAT_BINDER_FLAGS
        // obj.flags = 0x7f | FLAT_BINDER_FLAGS_ACCEPT_FDS;
        if let Some(handle) = handle {
            obj.type_ = sys::BinderType::Handle as u32;
            obj.target.handle = handle.borrow().handle;
        } else {
            obj.type_ = sys::BinderType::Handle as u32;
        }
        self.write_object(obj, false).unwrap(); // TODO: Propagate error
    }

    fn write_object(&mut self, val: flat_binder_object, null_metadata: bool) -> BinderResult<()> {
        //self.write_type(val)
        if val.type_ == sys::BinderType::Fd as u32 {
            if !self.allow_fds {
                return Err(BinderErrorKind::FdsNotAllowed.into())
            } else {
                self.has_fds = Some(true);
            }
        }
        if null_metadata || unsafe { val.target.binder } != 0 {
            self.objects.push(self.data.position() as usize);
            // TODO: acquire_object
        }
        Ok(())
    }

    pub fn write_buf(&mut self, buf: &[u8]) {
        let padded = pad_size!(buf.len());
        self.data.write(buf).expect("Write bigger than usize");
        // TODO: We should make this fail *hard* if there isn't enough data !!
        if padded -  buf.len() > 0 {
            self.data.seek(SeekFrom::Current((padded - buf.len() - 1) as i64))
                .expect("Should never seek to a negative index");
            self.data.write(&[0]).expect("Write bigger than usize");
        }
    }

    // TODO: readValue/writeValue
}

impl Parcel for OwnedParcel {
    fn data(&mut self) -> &mut Reader {
        &mut self.data
    }

    fn objects(&self) -> &[usize] {
        &self.objects[..]
    }

    fn conn_mut(&mut self) -> &mut BinderConnection {
        &mut self.conn
    }

    fn has_data(&self) -> bool {
        self.data.position() < self.data.get_ref().len() as u64
    }

    fn as_data_slice_mut(&mut self) -> &mut [u8] {
        self.data.get_mut()
    }
    fn as_objects_slice_mut(&mut self) -> &mut [usize] {
        &mut self.objects
    }
}

// TODO: Is it *const or *mut ???
pub unsafe fn create_binder_parcel<'a>(binder: BinderConnection, data: *mut u8, data_len: usize, offsets: *mut usize, offset_len: usize) -> (BinderParcel<'a>) {
    println!("Creating binderparcel with data_len {}", data_len);
    BinderParcel {
        data: Cursor::new(std::slice::from_raw_parts_mut(data, data_len)),
        offsets: std::slice::from_raw_parts_mut(offsets, offset_len),
        conn: binder
    }
}

#[derive(Debug)]
pub struct BinderParcel<'a> {
    data: Cursor<&'a mut [u8]>,
    offsets: &'a mut [usize],
    conn: BinderConnection
}

impl<'a> Parcel for BinderParcel<'a> {
    fn data(&mut self) -> &mut Reader {
        &mut self.data
    }

    fn objects(&self) -> &[usize] {
        &self.offsets[..]
    }

    fn conn_mut(&mut self) -> &mut BinderConnection {
        &mut self.conn
    }

    fn has_data(&self) -> bool {
        self.data.position() < self.data.get_ref().len() as u64
    }

    fn as_data_slice_mut(&mut self) -> &mut [u8] {
        self.data.get_mut()
    }
    fn as_objects_slice_mut(&mut self) -> &mut [usize] {
        &mut self.offsets
    }
}

impl<'a> Drop for BinderParcel<'a> {
    fn drop(&mut self) {
        let _ = self.conn.free_buffer(self.data.get_mut().as_mut_ptr());
    }
}

