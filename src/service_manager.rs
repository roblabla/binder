//!
//! Service Manager
//!
//! On a default Android installation, the Service Manager is the first binder
//! service you'll communicate with. It acts as a kind of directory for all
//! other system-level services, and as a gateway, implementing some simple
//! permission checking.
//!
//! In most cases you'll only use the Service Manager to do one thing : get a
//! connection to the Activity Manager service, which handles app-level
//! services. However, it can be used for other purposes, given special
//! privileges : you can get access to other system-level services such as the
//! telephony services, or you can even register your own system-level service.
//!

use std::rc::Rc;
use std::cell::RefCell;
use {Handle, OwnedParcel, IInterface, BinderResult, FIRST_CALL_TRANSACTION};
use parcel::Parcel;

pub struct ServiceManager {
    handle: Rc<RefCell<Handle>>
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum ServiceManagerProtocol {
    #[allow(dead_code)] // TODO: Remove allow(dead_code)
    GetService = FIRST_CALL_TRANSACTION,
    CheckService,
    AddService,
    ListServices
}

impl ServiceManager {
    /*/// Retrieve an existing service, blocking for a few seconds if it doesn't
    /// yet exist
    fn getService<T>(name: &str) -> Handle {
        // TODO: basically call check service in loop
    }*/

    // TODO: I don't need to pass name, I can just use T::get_interface_descriptor() !
    /// Retrieve an existing service
    pub fn check_service<T: IInterface>(&mut self, name: &str) -> BinderResult<Option<T>> {
        let mut data = OwnedParcel::new(self.handle.borrow().conn.clone());
        data.write_interface_token(ServiceManager::get_interface_descriptor());
        data.write_string16(name);
        let mut reply = self.handle.borrow_mut().transact(ServiceManagerProtocol::CheckService as u32, &mut data, 0)?;
        Ok(reply.read_strong_binder()?.map(|e| T::from_handle(e)))
    }

    // TODO: Finish this. Instead of a handle, this should take the eventual
    // Binder enum (that can contain either a Local binder or a Handle).
    pub fn add_service(&mut self, name: &str, handle: Rc<RefCell<Handle>>, allow_isolated: bool) -> BinderResult<()> {
        let mut data = OwnedParcel::new(self.handle.borrow().conn.clone());
        data.write_interface_token(ServiceManager::get_interface_descriptor());
        data.write_string16(name);
        data.write_strong_binder(Some(handle));
        data.write_i32(if allow_isolated { 1 } else { 0 });
        self.handle.borrow_mut().transact(ServiceManagerProtocol::AddService as u32, &mut data, 0).unwrap();
        panic!("Not implemented yet");
    }

    /// List all registered system services.
    pub fn list_services(&mut self) -> Vec<String> {
        let mut data = OwnedParcel::new(self.handle.borrow().conn.clone());
        let mut res = Vec::new();
        for i in 0.. {
            data.clear();
            data.write_interface_token(Self::get_interface_descriptor());
            data.write_i32(i);
            println!("Reading service {}", i);
            match self.handle.borrow_mut().transact(ServiceManagerProtocol::ListServices as u32, &mut data, 0) {
                Ok(mut reply) => {
                    let service = reply.read_string16().expect("read_string16 should work !");
                    println!("Got a service {}", service);
                    res.push(service);
                },
                Err(_) => break
            }
        }
        res
    }
}

impl IInterface for ServiceManager {
    fn get_interface_descriptor() -> &'static str {
        "android.os.IServiceManager"
    }
    fn from_handle(handle: Rc<RefCell<Handle>>) -> ServiceManager {
        ServiceManager { handle: handle }
    }
}
