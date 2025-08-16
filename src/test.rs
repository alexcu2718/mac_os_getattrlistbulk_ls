#[test]
fn show_weird_behaviour_of_macos(){
    use super::*;

    assert!(VREG!=libc::DT_REG);
    assert!(VDIR!=libc::DT_DIR);
    assert!(VBLK!=libc::DT_BLK);
    assert!(VFIFO!=libc::DT_FIFO);
    assert!(VSOCK!=libc::DT_SOCK);




}