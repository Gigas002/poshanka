use std::io::Read;

/// The handler only writes to the pipe: raising SIGINT makes it readable
/// and the process goes on.
#[test]
fn a_signal_makes_the_descriptor_readable() {
    let stop = super::stop_signals().expect("handlers installed");
    let mut pipe = std::fs::File::from(stop);
    // SAFETY: raise(3) delivers SIGINT to this thread, whose handler is ours.
    assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
    let mut byte = [0u8; 1];
    pipe.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [1]);
}
