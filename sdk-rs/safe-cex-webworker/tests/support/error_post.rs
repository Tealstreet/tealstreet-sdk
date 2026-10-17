pub fn unexpected() -> safe_cex_webworker::error_post::WorkerErrorPost {
    use safe_cex_webworker::error_post::WorkerErrorPost;
    use std::rc::Rc;
    WorkerErrorPost::new(
        Rc::new(|_, _| panic!("unexpected market timeout Error creation")),
        Rc::new(|_| panic!("unexpected market timeout post")),
    )
}
