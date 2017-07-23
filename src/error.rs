mod binder {
    use libc;

    error_chain! {
        errors {
            NoMemory {
                description("Out of memory")
                display("Out of memory")
            }
            InvalidOperation
            BadValue
            BadType
            NameNotFound
            PermissionDenied
            NoInit
            AlreadyExists
            DeadObject
            FailedTransaction
            BadIndex
            NotEnoughData
            WouldBlock
            TimedOut
            UnknownTransaction
            FdsNotAllowed
            // TODO: Maybe this is not necessary ? I handle NULL with Option
            UnexpectedNull
            UnknownError(code: i32) {
                description("Unknown error")
                display("Unknown error code {}", code)
            }
        }
    }

    impl From<libc::c_int> for Error {
        fn from(err: libc::c_int) -> Error {
            use self::ErrorKind::*;
            const UNKNOWN_ERROR : i32 = (-2147483647-1);

            let kind =
                if err == -libc::ENOMEM           { NoMemory }
                else if err == -libc::ENOSYS      { InvalidOperation }
                else if err == -libc::EINVAL      { BadValue }
                else if err == UNKNOWN_ERROR + 1  { BadType }
                else if err == -libc::ENOENT      { NameNotFound }
                else if err == -libc::EPERM       { PermissionDenied }
                else if err == -libc::ENODEV      { NoInit }
                else if err == -libc::EEXIST      { AlreadyExists }
                else if err == -libc::EPIPE       { DeadObject }
                else if err == UNKNOWN_ERROR + 2  { FailedTransaction }
                // TODO: !defined(_WIN32)
                else if err == -libc::EOVERFLOW   { BadIndex }
                else if err == -libc::ENODATA     { NotEnoughData }
                else if err == -libc::EWOULDBLOCK { WouldBlock }
                else if err == -libc::ETIMEDOUT   { TimedOut }
                else if err == -libc::EBADMSG     { UnknownTransaction }
                // End todo
                else if err == UNKNOWN_ERROR + 7  { FdsNotAllowed }
                else if err == UNKNOWN_ERROR + 8  { UnexpectedNull }
                else if err < 0                   { UnknownError(err) }
                else { panic!("From<libc::c_int> for Error called with positive value {}", err) };
            kind.into()
        }

        // TODO: Write conversion for nix::Errno
    }
}

/*mod exception {
    error_chain! {
        errors {
            Security
            BadParcelable
            IllegalArgument
            NullPointer
            IllegalState
            NetworkMainThread
            UnsupportedOperation
            ServiceSpecific
            RuntimeException(code: i32, s: String) {
                description("Unknown exception code")
                display("Unknown exception code: {} msg {}", code, s)
            }
        }
    }
}*/

pub use self::binder::{Error as BinderError, ErrorKind as BinderErrorKind, Result as BinderResult};

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Nix(::nix::Error);
    }
    links {
        Binder(binder::Error, binder::ErrorKind);
    }
    errors {
        WrongProtocolVersion {
            description("Binder driver protocol does not match user space protocol!")
            display("Binder driver protocol does not match user space protocol!")
        }
    }
}
