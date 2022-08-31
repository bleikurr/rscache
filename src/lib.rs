use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::{Instant, Duration};


struct AsyncCacheData<T, S>
{
    data: Arc<T>,
    ttl: Duration,
    state: Option<S>,
    age: std::time::Instant,
    refresher: Box<dyn Fn(Option<&mut S>) -> T + Send + Sync>
}

impl<T, S> AsyncCacheData<T, S>
{
    fn get_ref(&self) -> Result<Arc<T>, ()> {
        if self.age.elapsed() > self.ttl {
            return Err(());
        }
        Ok(Arc::clone(&self.data))
    }

    fn refresh(&mut self) -> Arc<T> {
        if self.age.elapsed() < self.ttl { 
            return Arc::clone(&self.data);
        }

        match self.state.as_mut() {
            Some(state) => { 
                self.data = Arc::new(
                    (self.refresher)(Some(state))
                );
            }
            None => {
                self.data = Arc::new((self.refresher)(None));
            }
        }
        self.age = Instant::now();
        Arc::clone(&self.data)
    }
}

pub struct AsyncCache<T, S>
{
    data: RwLock<AsyncCacheData<T, S>>
}

impl<T, S> AsyncCache<T, S>
{
    pub fn new(mut state: Option<S>, ttl: Duration, refresher: Box<dyn Fn(Option<&mut S>) -> T + Send + Sync>)
        -> Self {
        let data = match state.as_mut() {
            Some(state) => refresher(Some(state)),
            None => refresher(None)
        };
        let cd = AsyncCacheData {
            data: Arc::new(data),
            age: Instant::now(),
            state,
            ttl,
            refresher
        };
        AsyncCache {
            data: RwLock::new(cd)
        }
    }

    pub async fn get_data(&self) -> Arc<T> {
        {
            let data = self.data.read().await;
            if let Ok(data) = data.get_ref() {
                return data;
            }
        }
        // Refresh if data is old
        let mut data = self.data.write().await;
        data.refresh()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread")]
    async fn cache_test() {
        struct State {
            i: u32
        }
        let s = State { i: 0 }; 


        let cache: ACache<u32, State> = Cache::new(Some(s), Duration::from_millis(50), Box::new(|s| {
            let s = s.unwrap();
            s.i += 1;
            s.i
        }));
        
        let cache1 = Cache::clone(&cache);
        let t1 = tokio::spawn( async move {
            let mut data = cache1.get_data().await;
            assert_eq!(*data, 1);
            sleep(Duration::from_millis(10)).await;
            data = cache1.get_data().await;
            assert_eq!(*data, 1);
            sleep(Duration::from_millis(50)).await;
            data = cache1.get_data().await;
            assert_eq!(*data, 2);
            sleep(Duration::from_millis(20)).await;
            data = cache1.get_data().await;
            assert_eq!(*data, 2);
            sleep(Duration::from_millis(50)).await;
            data = cache1.get_data().await;
            assert_eq!(*data, 3);
        });

        let cache2 = Cache::clone(&cache);
        let t2 = tokio::spawn( async move {
            let mut data = cache2.get_data().await;
            assert_eq!(*data, 1);
            sleep(Duration::from_millis(20)).await;
            data = cache2.get_data().await;
            assert_eq!(*data, 1);
            sleep(Duration::from_millis(40)).await;
            data = cache2.get_data().await;
            assert_eq!(*data, 2);
            sleep(Duration::from_millis(10)).await;
            data = cache2.get_data().await;
            assert_eq!(*data, 2);
            sleep(Duration::from_millis(50)).await;
            data = cache2.get_data().await;
            assert_eq!(*data, 3);

        });


        t1.await.unwrap();
        t2.await.unwrap();
    }
}

