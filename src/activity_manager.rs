//!
//! ActivityManager
//!

pub struct ActivityManager {
    handle: Rc<RefCell<Handle>>
}

// TODO: I need Intent and a few other things here...

pub struct Intent {
    mAction: String,
    mData: Uri,
    mType: String,
    mPackage: Option<String>,
    /*mComponent: ComponentName,*/
    mFlags: u32,
    mCategories: HashSet<String>,
    /*mExtras: Bundle,*/
    /*mSourceBounds: Rect,*/
    mSelector: Option<Intent>,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum ActivityManagerProtocol {
    StartActivity = FirstCallTransaction + 2,
}


        /*mAm.broadcastIntent(None, intent, None, receiver, 0, null, null, requiredPermissions,
                android.app.AppOpsManager.OP_NONE, null, true, false, mUserId);*/

impl ActivityManager {
    /// Start an activity.
    ///
    /// # Example
    ///
    /// ```
    /// # let conn = BinderConnection::open().unwrap();
    /// # let svcmgr = conn.get_service_manager().unwrap();
    /// # let actmgr = svcmgr.check_service(ActivityManager::get_interface_descriptor());
    ///
    /*pub fn start_activity(caller: Option<()/*IApplicationThread*/>, callingPackage: Option<&str>, intent: Intent, resolvedType: Option<&str>, resultTo: Option<IBinder>, resultWho: Option<&str>, requestCode: i32, flags: i32, profilerInfo: Option<()/*ProfilerInfo*/>, options: Option<()/*Bundle*/>) -> BinderResult<i32> {
        // TODO: In java, parcels are taken from a Pool. This could be a nice
        // idea for performance ?
        let mut data = OwnedParcel::new(self.handle.borrow().conn.clone());

        data.write_interface_token(Self::get_interface_descriptor());
        data.write_strong_binder(caller.map(|c| c.as_binder));
        data.write_string16(callingPackage);
        intent.write_to_parcel(data);
        data.write_string16(resolvedType);
        data.write_strong_binder(resultTo);
        data.write_string16(resultWho);
        data.write_i32(requestCode);
        data.write_i32(startFlags);
        if let Some(profilerInfoInner) = profilerInfo {
            data.write_i32(1);
            profilerInfoInner.write_to_parcel(data, Parcelable.PARCELABLE_WRITE_RETURN_VALUE);
        } else {
            data.write_i32(0);
        }
        if let Some(optionsInner) = options {
            data.write_i32(1);
            optionsInner.write_to_parcel(data, 0);
        } else {
            data.write_i32(0);
        }
        let reply = self.handle.borrow_mut().transact(ActivityManagerProtocol::StartActivity as u32, &mut data, 0)?;
        let exception = reply.read_exception();
        reply.read_i32()
    }

    pub fn broadcast_intent(/*caller: Option<IApplicationThread>*/, intent: Intent, resolvedType: Option<&str>, resultTo: IIntentReceiver, resultCode: i32, resultData: Option<&str>, map: Option<Bundle>, requiredPermissions: &[&str], appOp: i32, options: Option<Bundle>, serialized: bool, sticky: bool, userId: i32) {
        
    }*/
}

impl IInterface for ActivityManager {
    fn get_interface_descriptor() -> &'static str {
        "android.app.IActivityManager"
    }

    fn from_handle(handle: Rc<RefCell<Handle>>) -> Self {
        ActivityManager { handle: handle }
    }
}
