extern crate binder;

#[test]
fn it_works() {
    // Binder connection is unbound, meaning it doesns't talk to anybody at first.
    println!("Opening binder connection");
    let mut binder = binder::BinderConnection::open().unwrap();
    println!("Getting service manager object");
    let mut svcmgr = binder.get_service_manager().unwrap();
    println!("Listing services");
    println!("{:?}", svcmgr.list_services());
}
