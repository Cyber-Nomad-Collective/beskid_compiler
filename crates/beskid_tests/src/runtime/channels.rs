use beskid_runtime::{
    channel_close, channel_create, channel_receive, channel_send, channel_try_receive,
    channel_try_send, run_closure_as_main, status::{STATUS_CLOSED, STATUS_OK, STATUS_WOULD_BLOCK},
};

#[test]
fn channel_unbounded_send_receive() {
    run_closure_as_main(|| {
        let ch = channel_create(0, 0);
        assert_eq!(channel_send(ch, 42), STATUS_OK);
        let mut out = 0i64;
        assert_eq!(channel_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 42);
        0
    });
}

#[test]
fn channel_try_operations() {
    run_closure_as_main(|| {
        let ch = channel_create(0, 0);
        assert_eq!(channel_try_send(ch, 1), STATUS_OK);
        let mut out = 0i64;
        assert_eq!(channel_try_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 1);
        assert_eq!(channel_try_receive(ch, &mut out), STATUS_WOULD_BLOCK);
        0
    });
}

#[test]
fn channel_close_drains_then_closed() {
    run_closure_as_main(|| {
        let ch = channel_create(0, 0);
        assert_eq!(channel_send(ch, 7), STATUS_OK);
        channel_close(ch);
        let mut out = 0i64;
        assert_eq!(channel_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 7);
        assert_eq!(channel_receive(ch, &mut out), STATUS_CLOSED);
        assert_eq!(channel_send(ch, 1), STATUS_CLOSED);
        0
    });
}
