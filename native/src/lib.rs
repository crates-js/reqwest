use neon::prelude::*;
use once_cell::sync::Lazy;
use std::{future::Future, cell::RefCell, pin::Pin};
use reqwest::Client;
use tokio::sync::oneshot;

static RT: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .unwrap()
});

struct FutureTask<T, O: Value, E = ()> where T: IntoJsType<Output = O> {
    future: RefCell<Option<Pin<Box<dyn Future<Output = Result<T, E>> + Send>>>>,
    output: std::marker::PhantomData<O>,
}

impl<T, O, E> FutureTask<T, O, E>
where
    T: IntoJsType<Output = O> + Send + 'static,
    O: Value,
    E: Send + AsRef<str> + 'static
{
    pub fn new(fut: impl Future<Output = Result<T, E>> + Send + 'static) -> Self {
        Self {
            future: RefCell::new(Some(Box::pin(fut))),
            output: std::marker::PhantomData
        }
    }
}

trait IntoJsType {
    type Output: Value;

    fn into_js<'a>(self, cx: &mut TaskContext<'a>) -> neon::handle::Handle<'a, Self::Output>;
}

impl<T, O, E> Task for FutureTask<T, O, E>
where
    T: std::fmt::Debug + IntoJsType<Output = O> + Send + 'static,
    O: Value,
    E: std::fmt::Debug + AsRef<str> + Send + 'static
{
    type Output = T;
    type Error = E;
    type JsEvent = O;

    fn perform(&self) -> Result<Self::Output, Self::Error> {
        let fut = self.future.borrow_mut().take().unwrap();
        RT.handle().block_on(fut)
    }

    fn complete<'a>(self, mut cx: TaskContext<'a>, result: Result<Self::Output, Self::Error>) -> JsResult<Self::JsEvent> {
        match result {
            Ok(v) => Ok(v.into_js(&mut cx)),
            Err(e) => cx.throw_error(e)
        }
    }
}

declare_types! {
    pub class JsClient for Client {
        init(mut _cx) {
            Ok(Client::new())
        }

        method get(mut cx) {
            let url = cx.argument::<JsString>(0)?;
            let f = cx.argument::<JsFunction>(1)?;
            let url: String = url.value();

            let ret = cx.undefined().upcast();

            let this = cx.this();
            let guard = cx.lock();
            let client = this.borrow(&guard);

            let request = client.get(&url).build().unwrap();
            let fut = client.execute(request);

            let task = FutureTask::new(async move {
                let (tx, rx) = oneshot::channel();

                RT.handle().spawn(async move {
                    let result = fut.await;
                    tx.send(result).unwrap();
                });

                match rx.await.unwrap() {
                    Ok(v) => Ok(v.text().await.unwrap()),
                    Err(e) => Err(format!("{}", e))
                }
            });

            task.schedule(f);
            Ok(ret)
        }
    }
}

impl IntoJsType for String {
    type Output = JsString;

    fn into_js<'a>(self, cx: &mut TaskContext<'a>) -> Handle<'a, Self::Output> {
        cx.string(self)
    }
}

fn get(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let url = cx.argument::<JsString>(0)?;
    let f = cx.argument::<JsFunction>(1)?;
    let url: String = url.value();

    let task = FutureTask::new(async {
        let (tx, rx) = oneshot::channel();

        RT.handle().spawn(async move {
            let result = reqwest::get(&url).await;
            tx.send(result).unwrap();
        });

        match rx.await.unwrap() {
            Ok(v) => Ok(v.text().await.unwrap()),
            Err(e) => Err(format!("{}", e))
        }
    });

    task.schedule(f);
    Ok(cx.undefined())
}

register_module!(mut m, {
    m.export_class::<JsClient>("Client")?;
    m.export_function("get", get)
});