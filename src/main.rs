mod http;
mod runtime;
use tokio::runtime::Runtime;

fn main() {
    let mut executor = runtime::init();
    executor.block_on(async_main());
}


async fn async_main() {
    let rt = Runtime::new().unwrap();
    let _guard = rt.enter();
    println!("Program starting");
    let url = "http://127.0.0.1:8080/600/HelloAsyncAwait1";
    let res = reqwest::get(url).await.unwrap();
    let txt = res.text().await.unwrap();
    println!("{txt}");
    let url = "http://127.0.0.1:8080/400/HelloAsyncAwait2";
    let res = reqwest::get(url).await.unwrap();
    let txt = res.text().await.unwrap();
    println!("{txt}");
}
