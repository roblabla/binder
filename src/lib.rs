//!
//! Binder
//!
//! This is a rewrite of the libbinder userspace library to take full advantage
//! of Rust's architecture.
//!
//! ```
//! # fn main() {
//! let connection = BinderConnection::open().unwrap();
//! let svcmgr = connection.get_context_object();
//! let actmgr : ActivityManager = svcmgr.get_service("android.os.IActivityManager");
//!
//! let intent = Intent::new(intent::ACTION_VIEW, "http://www.google.com");
//! actmgr.start_activity(None, None, intent, None, )
//! # }
//! ```
//!
//! In the C++ framework, it is possible to abstract over multiple kinds of
//! Binder (BpBinder, which represents a Binder in a remote process, and
//! BbBinder, which represents a local Binder service). This library doesn't
//! even try to do this. Instead, we focus on the BpBinder aspect (for now).
//!
#![feature(conservative_impl_trait)]
#![feature(const_fn)]
#![feature(associated_consts)]
#[macro_use]
extern crate log;
extern crate libc;
// TODO: Use memmap crate instead of nix. It honestly looks a lot better, using
// File object and supporting Drop, etc...
#[macro_use]
extern crate nix;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate bitflags;
extern crate encoding;
extern crate byteorder;

pub mod sys;
pub mod error;
pub mod parcel;
pub mod service_manager;

use std::cell::RefCell;
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::mem::size_of;

use service_manager::ServiceManager;
use error::*;
use parcel::*;

const BINDER_VM_SIZE : usize = (1024 * 1024) - (4096 * 2);

// This will be passed around in the various places that need it
#[derive(Debug)]
struct BinderConnectionInner {
    fd: RawFd,
    vm_start: *mut nix::libc::c_void,
    /*out: parcel::OwnedParcel,
    _in: parcel::OwnedParcel,*/
    handle_map: HashMap<u32, Weak<RefCell<Handle>>>
}

impl Drop for BinderConnectionInner {
    fn drop(&mut self) {
        let _ = nix::unistd::close(self.fd);
        let _ = unsafe { nix::sys::mman::munmap(self.vm_start, BINDER_VM_SIZE) };
    }
}

/// A connection to the Binder kernel interface.
///
/// The Binder connection doesn't actually allow to talk to other applications
/// directly. Instead, you should first get an IBinder to the Service Manager by
/// calling `get_context_object()`. You will then be able to access other
/// services through the Service Manager.
// TODO: Rc/Arc the BinderConnection ?
#[derive(Debug, Clone)]
pub struct BinderConnection {
    inner: Rc<RefCell<BinderConnectionInner>>
}

// TODO: Expand the binder_structs
#[derive(Debug)]
enum ReturnProtocolValue {
    Ok,
    Error(i32),
    Transaction(sys::binder_transaction_data),
    Reply(sys::binder_transaction_data),
    AcquireResult(i32), // What is this ? A handle ?
    DeadReply,
    TransactionComplete,
    // TODO: This should really be { ptr, cookie } directly
    IncRefs(sys::binder_ptr_cookie),
    Acquire(sys::binder_ptr_cookie),
    Release(sys::binder_ptr_cookie),
    DecRefs(sys::binder_ptr_cookie),
    AttemptAcquire(sys::binder_pri_ptr_cookie),
    Noop,
    SpawnLooper,
    Finished,
    DeadBinder(sys::binder_uintptr_t),
    ClearDeathNotificationDone(sys::binder_uintptr_t),
    FailedReply
}

// Represents a connection to a service. The connection can be Local or Remote.
// enum IBinder {
//     Local,
//     Remote
// }

fn parse_one<T: Parcel>(_in: &mut T) -> Option<ReturnProtocolValue> {
    use sys::ReturnProtocol::*;
    match _in.read_i32() {
        Err(ref err) if err.kind() == std::io::ErrorKind::UnexpectedEof => None,
        Err(_) => panic!("This should never happen !"),
        std::result::Result::Ok(cmd) => match sys::ReturnProtocol::from_primitive(cmd) {
            Some(Ok) => Some(ReturnProtocolValue::Ok),
            Some(Error) => Some(ReturnProtocolValue::Error(_in.read_i32().expect("Binder protocol error"))),
            Some(x) if x == Transaction || x == Reply => {
                // FIXME: constexpr size_of
                let mut buf = [0; size_of::<sys::binder_transaction_data>()];
                _in.read_buf(&mut buf).expect("Binder protocol error");
                let txn : sys::binder_transaction_data = unsafe { std::mem::transmute(buf) };
                if x == Transaction {
                    Some(ReturnProtocolValue::Transaction(txn))
                } else {
                    Some(ReturnProtocolValue::Reply(txn))
                }
            },
            Some(AcquireResult) => Some(ReturnProtocolValue::AcquireResult(_in.read_i32().expect("Binder protocol error"))),
            Some(DeadReply) => Some(ReturnProtocolValue::DeadReply),
            Some(TransactionComplete) => Some(ReturnProtocolValue::TransactionComplete),
            Some(x) if x == IncRefs || x == Acquire || x == Release || x == DecRefs => {
                // FIXME: constexpr size_of
                let mut buf = [0; size_of::<sys::binder_ptr_cookie>()];
                _in.read_buf(&mut buf).expect("Binder protocol error");
                let ptr : sys::binder_ptr_cookie = unsafe { std::mem::transmute(buf) };
                match x {
                    IncRefs => Some(ReturnProtocolValue::IncRefs(ptr)),
                    Acquire => Some(ReturnProtocolValue::Acquire(ptr)),
                    Release => Some(ReturnProtocolValue::Release(ptr)),
                    DecRefs => Some(ReturnProtocolValue::DecRefs(ptr)),
                    _ => unreachable!()
                }
            },
            Some(AttemptAcquire) => {
                // FIXME: constexpr size_of
                let mut buf = [0; size_of::<sys::binder_pri_ptr_cookie>()];
                _in.read_buf(&mut buf).expect("Binder protocol error");
                let ptr : sys::binder_pri_ptr_cookie = unsafe { std::mem::transmute(buf) };
                Some(ReturnProtocolValue::AttemptAcquire(ptr))
            },
            Some(Noop) => Some(ReturnProtocolValue::Noop),
            Some(SpawnLooper) => Some(ReturnProtocolValue::SpawnLooper),
            Some(Finished) => Some(ReturnProtocolValue::Finished),
            Some(x) if x == DeadBinder || x == ClearDeathNotificationDone => {
                // FIXME: constexpr size_of
                let mut buf = [0; size_of::<sys::binder_uintptr_t>()];
                _in.read_buf(&mut buf).expect("Binder protocol error");
                let ptr : sys::binder_uintptr_t = unsafe { std::mem::transmute(buf) };
                if x == DeadBinder {
                    Some(ReturnProtocolValue::DeadBinder(ptr))
                } else {
                    Some(ReturnProtocolValue::ClearDeathNotificationDone(ptr))
                }
            },
            Some(FailedReply) => Some(ReturnProtocolValue::FailedReply),
            None => panic!("Unknown Binder command"), // TODO: Protocol Error
            _ => unreachable!()
        }
    }
}

pub trait IInterface {
    fn get_interface_descriptor() -> &'static str;
    fn from_handle(handle: Rc<RefCell<Handle>>) -> Self;
}

/// A connection to a Binder Service. Roughly equivalent to an sp<BpBinder> in
/// the libbinder framework.
///
/// In Binder parlance, there are two ways to be "connected" to a binder
/// Service : We can have a Strong Pointer to a BbBinder instance, or we can
/// have a Handle. In the first case, we say the Binder is "local", meaning it
/// hasn't crossed process boundary. In the later case, we say the Binder is
/// remote. When we send a Binder to another process, the kernel automatically
/// transforms the pointer into a Handle. See
/// https://github.com/torvalds/linux/blob/master/drivers/android/binder.c#L1594
///
/// For now, the only way to get one is to call
/// [BinderConnection.get_context_object](struct.BinderConnection.html#method.get_context_object)
#[derive(Debug)]
pub struct Handle {
    handle: u32,
    conn: BinderConnection
}

impl Handle {
    // TODO: conn->incStrongHandle()
    fn new(conn: BinderConnection, handle: u32) -> Handle {
        Handle {
            handle: handle,
            conn: conn
        }
    }

    // TODO: Take a &mut OwnedParcel for the reply ?
    // TODO: Why does T need 'a ?
    pub fn transact<'a, T: 'a + Parcel>(&mut self, code: u32, data: &mut T, flags: u32) -> BinderResult<impl Parcel + 'a> {
        // TODO: mAlive
        self.conn.call(self.handle, code, data, flags)
    }

    // TODO: Implement get_interface_descriptor()
}

// TODO: conn->decStrongHandle()
impl Drop for Handle {
    fn drop(&mut self) {

    }
}

// TODO: Of those, only Ping and Interface are used. The other three are
// basically deprecated as far as I can tell. ShellCommand always returns
// InvalidOperation, Dump returns NoError and Sysprops... I don't even know what
// it's supposed to do.
#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum BinderProtocol {
    PingTransaction = sys::pack_chars(b'_', b'P', b'N', b'G'),
    #[allow(dead_code)] // TODO: remove allow(dead_code)
    DumpTransaction = sys::pack_chars(b'_', b'D', b'M', b'P'),
    #[allow(dead_code)] // TODO: remove allow(dead_code)
    ShellCommandTransaction = sys::pack_chars(b'_', b'C', b'M', b'D'),
    #[allow(dead_code)] // TODO: remove allow(dead_code)
    InterfaceTransaction = sys::pack_chars(b'_', b'N', b'T', b'F'),
    #[allow(dead_code)] // TODO: Remove allow(dead_code)
    SyspropsTransaction = sys::pack_chars(b'_', b'S', b'P', b'R')
}

// TODO: This should go somewhere else...
pub const FIRST_CALL_TRANSACTION : u32 = 1;

// TODO: Add a non-blocking mode with mio, integrate with tokio !
impl BinderConnection {
    ///
    /// Attempts to connection a connection to the Binder Kernel Interface.
    ///
    /// # Errors
    ///
    /// This function will return :
    ///
    /// - `WrongProtocolVersion` if the kernel binder driver implements a
    ///   different protocol version
    /// - `Io` if there is an error opening the connection to the driver
    /// - `Nix` if there is an error mmapping the Binder VM
    pub fn open() -> Result<BinderConnection> {
        use nix::sys::mman::*;
        use std::os::unix::io::IntoRawFd;

        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/binder")?.into_raw_fd();

        let mut vers : sys::binder_version = unsafe { std::mem::zeroed() };
        unsafe { sys::binder_version(fd, &mut vers)?; }
        if vers.protocol_version != sys::CurrentProtocolVersion {
            error!("Binder driver protocol does not match user space protocol!");
            let _ = nix::unistd::close(fd);
            return Err(ErrorKind::WrongProtocolVersion.into())
        }
        // So, I need to make sure my theory is right, but I *think* binder
        // returns pointer to stuff in this zone when it needs to. This means
        // I need to find those pointer, and bind their lifetime to
        // BinderConnection...
        let map = unsafe { mmap(std::ptr::null_mut(), BINDER_VM_SIZE, PROT_READ, MAP_PRIVATE | MAP_NORESERVE, fd, 0)? };
        Ok(BinderConnection {
            inner: Rc::new(RefCell::new(BinderConnectionInner {
                fd: fd,
                vm_start: map,
                handle_map: HashMap::default()
            }))
        })
    }

    /// Gets a `Handle` to the current context object, or None if it wasn't
    /// registered yet.
    fn get_context_object(&mut self) -> Option<Rc<RefCell<Handle>>> {
        // TODO: a create_parcel() might be cool here
        let mut data = OwnedParcel::new(self.clone());
        // Make sure the context object exists ! In a standard android install,
        // this would be the ServiceManager. However, very early at boot, the
        // ServiceManager might not have already been started by init. If this
        // happens, we should return None instead of a broken Handle.
        if let Err(BinderError(BinderErrorKind::DeadObject, _)) = self.call(0, BinderProtocol::PingTransaction as u32, &mut data, 0) {
            return None
        }
        println!("Got context object");
        // TODO: Should I incStrong here ?
        Some(Rc::new(RefCell::new(Handle::new(self.clone(), 0))))
    }


    /// Gets a handle to the ServiceManager, or `None` if it wasn't registered
    /// as a context object yet. This can happen very early at boot, when the
    /// ServiceManager service hasn't been started yet.
    ///
    /// Note that if your context object is not a ServiceManager (for instance,
    /// if you're not on Android), you should use `get_context_object()`
    /// instead.
    pub fn get_service_manager(&mut self) -> Option<ServiceManager> {
        self.get_context_object().map(|h| ServiceManager::from_handle(h))
    }

    /// Get a strong reference to the given handle, creating the smart Handle
    /// object if it hasn't been created yet.
    fn get_strong_proxy_for_handle(&mut self, handle: u32) -> Rc<RefCell<Handle>> {
        // If we already have a handle, upgrade to a strong handle and return
        // the reference
        if let Some(e) = self.inner.borrow().handle_map.get(&handle).and_then(|e| e.upgrade()) {
            return e
        }

        // Otherwise, create a new reference to the handle and save a weak ref
        // in handle_map
        let e = Rc::new(RefCell::new(Handle::new(self.clone(), handle)));
        self.inner.borrow_mut().handle_map.insert(handle, Rc::downgrade(&e));
        e
    }

    // TODO: Why does this not take just some raw &mut [u8] ? I mean, parcel is
    // not *technically* required here
    fn binder_send_receive_bufs<'out, '_in>(&self, out_opt: Option<&'out mut Parcel>, mut in_opt: Option<&'_in mut OwnedParcel>) {
        let mut bwr : sys::binder_write_read = unsafe { std::mem::zeroed() };

        // The write_buffer is never written to in the kernel code, so having a
        // const reference is OK here.
        if let Some(out) = out_opt {
            bwr.write_size = out.as_data_slice_mut().len() as sys::binder_size_t;
            bwr.write_buffer = out.as_data_slice_mut().as_ptr() as sys::binder_uintptr_t;
        }
        {
            if let Some(ref mut _in) = in_opt {
                bwr.read_size = _in.capacity() as sys::binder_size_t;
                // This is only necessary until the end of the ioctl call just
                // bellow. I'd love it if it could actually have a lifetime.
                bwr.read_buffer = _in.as_data_slice_mut().as_ptr() as sys::binder_uintptr_t;
                bwr.read_consumed = 0;
            }
            trace!("Calling binder_write_read with bwr write_size = {}, read_size = {}", bwr.write_size, bwr.read_size);
            unsafe {
                // TODO: Map this to a BinderError
                // TODO: Loop on -eintr
                sys::binder_write_read(self.inner.borrow().fd, &mut bwr)
                    .expect("TODO: Figure out IOCTL error codes");
            }
        };

        if let Some(_in) = in_opt {
            unsafe { _in.set_data_len(bwr.read_consumed as usize) };
            _in.set_position(0);
        }
    }

    // TODO: Does it really need &mut ? What about &mut Parcel
    // TODO: This should return a BinderResult.
    fn call<'a, T: Parcel>(&mut self, handle: u32, code: u32, msg: &mut T, _/* TODO: flags */: u32) -> BinderResult<impl Parcel + 'a> {
        // TODO: In rust-binder, you can never have an error at the Parcel level
        // Ensure that is true !

        // TODO: flags |= TF_ACCEPT_FDS

        let mut data : sys::binder_transaction_data = unsafe { std::mem::zeroed() };
        data.target.handle = handle;
        data.code = code;
        // In the linux kernel, the pointer is casted to a (const void __user *)
        // If I understand things correctly, this means it's OK to just use a
        // non-mutable reference
        data.data_size = msg.as_data_slice_mut().len() as sys::binder_size_t;
        data.offsets_size = msg.as_objects_slice_mut().len() as sys::binder_size_t;
        // TODO: Support sending errors to the remote process. It's a bit weird
        // but there is this thing called the statusBuffer ?
        data.buffer = msg.as_data_slice_mut().as_ptr() as sys::binder_uintptr_t;
        data.offsets = msg.as_objects_slice_mut().as_ptr() as sys::binder_uintptr_t;

        let mut out = OwnedParcel::new(self.clone());
        let mut _in = OwnedParcel::new(self.clone());

        out.write_u32(sys::CommandProtocol::Transaction as u32);
        unsafe {
            out.write_buf(&std::mem::transmute::<sys::binder_transaction_data, [u8; size_of::<sys::binder_transaction_data>()]>(data));
        }

        self.binder_send_receive_bufs(Some(&mut out), Some(&mut _in));
        loop {
            loop {
                let one = parse_one(&mut _in);
                println!("Received one {:?}", one);
                match one {
                    Some(ReturnProtocolValue::TransactionComplete) => {
                        // TODO: if !reply && !acquireResult => break,
                    },
                    Some(ReturnProtocolValue::DeadReply) => {
                        return Err(BinderErrorKind::DeadObject.into())
                    },
                    Some(ReturnProtocolValue::FailedReply) => {
                        return Err(BinderErrorKind::FailedTransaction.into())
                    },
                    // TODO: AcquireResult => Needs BinderRc
                    Some(ReturnProtocolValue::Reply(txn)) => {
                        let mut buffer = unsafe {
                            parcel::create_binder_parcel(self.clone(), txn.buffer as *mut u8,
                                          txn.data_size as usize,
                                          txn.offsets as *mut usize,
                                          txn.offsets_size as usize)
                        };
                        if txn.flags & sys::TransactionFlags::STATUS_CODE.bits() == 0 {
                            println!("Returning from call");
                            return Ok(buffer)
                        } else {
                            let err = buffer.read_i32().expect("There should always be an error code in case of error.");
                            // TODO: Map to BinderResult
                            return Err(BinderErrorKind::UnknownError(err).into())
                        }
                    },
                    Some(val) => self.execute_command(val)?,
                    None => break
                }
            }
            self.binder_send_receive_bufs(None, Some(&mut _in));
        }
    }

    fn execute_command(&mut self, cmd: ReturnProtocolValue) -> BinderResult<()> {
        match cmd {
            ReturnProtocolValue::Error(x) => {
                // TODO: map x to BinderResult
                Err(BinderErrorKind::UnknownError(x).into())
            },
            ReturnProtocolValue::Ok => Ok(()),
            ReturnProtocolValue::Acquire(_) => {
                // TODO: Do stuff
                Ok(())
            },
            ReturnProtocolValue::Release(_) => {
                // push cookie to mPendingStrongDerefs
                Ok(())
            },
            ReturnProtocolValue::IncRefs(_) => {
                // TODO: increase ptr, reply
                Ok(())
            },
            ReturnProtocolValue::DecRefs(_) => {
                // Add cookie to mPendingWeakDerefs
                Ok(())
            },
            ReturnProtocolValue::AttemptAcquire(_) => {
                // attempt acquiring ptr. Make sure it's == obj.
                Ok(())
            },
            // Transact. We don't need this, for now. We will when we start
            // exposing services.
            ReturnProtocolValue::Transaction(_) => {
                // Transact. I wonder if it's possible to do this safely.
                // It looks like libbinder passes pointers around like they're
                // cookies. Pun intended.
                Ok(())
            },
            ReturnProtocolValue::DeadBinder(_) => {
                // send obituary
                // reply
                Ok(())
            },
            ReturnProtocolValue::ClearDeathNotificationDone(_) => {
                // getWeakRefs()->decWeak(proxy)
                Ok(())
            },
            ReturnProtocolValue::Finished => {
                Err(BinderErrorKind::TimedOut.into())
            },
            ReturnProtocolValue::Noop => Ok(()),
            ReturnProtocolValue::SpawnLooper => {
                // process->spawnPooledThread(false)
                Ok(())
            },
            cmd => panic!("Unexpected command {:?}", cmd)
        }
    }

    // Unless we talk with the driver, it shouldn't have to allocate stuff
    // to me anyway. So it's ok if we wait until the next call to send it.
    fn free_buffer(&mut self, buf: *mut u8) -> Result<()> {
        let mut out = OwnedParcel::new(self.clone());

        out.write_u32(sys::CommandProtocol::FreeBuffer as u32);
        out.write_pointer(buf as sys::binder_uintptr_t);
        self.binder_send_receive_bufs(Some(&mut out), None);
        Ok(())
    }

    /*fn talk_with_driver(&self, doReceive: bool) -> Result<()> {
        let bwr : binder_write_read = std::mem::zeroed();
        let err = {
            bwr.write_size = self.out.data.get_ref().len();
            bwr.write_buffer = self.out.data.get_mut().as_mut_ptr();
            if doReceive && self._in.data_pos >= self._in.data.len() {
                bwr.read_size = self._in.data.get_ref().capacity();
                bwr.read_buffer = self._in.data.get_mut().as_mut_ptr();
            }
            loop {
                err = sys::binder_write_read(self.fd, &mut bwr);
                match err {
                    nix::Error::Sys(Errno::EINTR) => continue,
                    _ => break
                }
            }
        if let Ok(_) = err {
            // Kind of crazy. Better be careful about this
            if bwr.write_consumed >= out.data.len() {
                out.set_data_size(0);
            } else {
                assert!("Something weird happen. In Android source, this is not supposed to happen. CF https://github.com/LineageOS/android_frameworks_native/blob/1eed0779cf/libs/binder/IPCThreadState.cpp#L888");
            if bwr.read_consumed > 0 {
                unsafe { _in.data.get_mut().set_len(bwr.read_consumed) };
                _in.data.set_position(0);
            }
        } else {
            err
        }
    }*/
}

// TODO: Develop BinderRc<T>

#[cfg(test)]
mod tests {
}
