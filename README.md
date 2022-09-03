# rusticache

Cache library to be used in async or singlethreaded(not implemented) context.

### Simple usage:

```
use std::sync::Arc;
use rusticache::AsyncCache;
use std::time::Duration;

async fn do_stuff() {
    let cache = Arc::new(AsyncCache::new(
        Duration::from_secs(10),
        Box::new(|| Ok(String::from("This is Sparta!")))
    ));

    let data = cache.get_data().await;
    assert_eq!(*data.unwrap(), String::from("This is Sparta!"));
}
do_stuff();
```


