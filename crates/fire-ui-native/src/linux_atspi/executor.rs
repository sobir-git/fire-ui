// Copyright 2024 The AccessKit Authors. All rights reserved.
// Licensed under the MIT license (found in the LICENSE-MIT file).
// Derived from zbus. Copyright 2024 Zeeshan Ali Khan, MIT licensed.

use std::future::Future;

pub(crate) use async_task::Task;

pub(crate) struct Executor<'a>(async_executor::Executor<'a>);

impl Executor<'_> {
    pub(crate) fn new() -> Self {
        Self(async_executor::Executor::new())
    }

    pub(crate) fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
        _name: &str,
    ) -> Task<T> {
        self.0.spawn(future)
    }

    pub(crate) async fn run<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.run(future).await
    }
}
