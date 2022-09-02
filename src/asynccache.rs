use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::{Instant, Duration};

pub type AsyncCacheResult<T> = Result<Arc<T>, Box<dyn std::error::Error + Send + Sync>>;
pub type AsyncRefreshResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Implementation of an inner struct for AsyncCache. Encapsulated.
/// 
/// T: Type of data being cached.
struct AsyncDataCache<T>
{
    data: Option<Arc<T>>,
    ttl: Duration,
    age: std::time::Instant,
    refresher: Box<dyn Fn() -> AsyncRefreshResult<T> + Send + Sync>
}

/// Implementation of inner struct of AsyncCache. Encapsulated.
/// 
/// T: Type of data being cached.
/// S: State used by 'refresher'.
struct AsyncDataCacheWithState<T, S>
{
    data: Option<Arc<T>>,
    ttl: Duration,
    state: S,
    age: std::time::Instant,
    refresher: Box<dyn Fn(&mut S) -> AsyncRefreshResult<T> + Send + Sync>
}


/// Trait used by encapsulated structs
trait DataCache {
    type Data;
    /// Returns the data from the cache. Error if data is old. Error handled by cache.
    fn get_reference(&self) -> Result<Arc<Self::Data>, ()>;// Err means data is old

    /// Returns cache data. Returns the error from the "refresher" function
    fn refresh(&mut self) -> AsyncCacheResult<Self::Data>;
    
 }

impl<T, S> DataCache for AsyncDataCacheWithState<T, S>
{
    type Data = T;
    fn get_reference(&self) -> Result<Arc<Self::Data>, ()> {
        if self.age.elapsed() > self.ttl {
            return Err(());
        }

        Ok(Arc::clone(self.data.as_ref().unwrap()))
    }

    fn refresh(&mut self) -> AsyncCacheResult<Self::Data> {
        if self.age.elapsed() < self.ttl { 
            return Ok(Arc::clone(self.data.as_ref().unwrap()));
        }

        let data = (self.refresher)(&mut self.state)?;
        self.data = Some(Arc::new(data));
        self.age = Instant::now();
        Ok(Arc::clone(self.data.as_ref().unwrap()))
    }
}

impl<T> DataCache for AsyncDataCache<T> 
{
    type Data = T; 
    fn get_reference(&self) -> Result<Arc<Self::Data>, ()> {
        if self.age.elapsed() > self.ttl {
            return Err(());
        }

        Ok(Arc::clone(self.data.as_ref().unwrap()))
    }

    fn refresh(&mut self) -> AsyncCacheResult<Self::Data> {
        if self.age.elapsed() < self.ttl { 
            return Ok(Arc::clone(self.data.as_ref().unwrap()));
        }
       
        eprintln!("Refreshing data!");
        let data = (self.refresher)()?;
        self.data = Some(Arc::new(data));
        self.age = Instant::now();
        Ok(Arc::clone(self.data.as_ref().unwrap()))
    }

}

/// T: Type of data being cached.
/// S: State used by 'refresher' function if needed. Can be () if not needed.
/// 
/// Best used with Arc
/// ```
/// use std::sync::Arc;
/// use rscache::AsyncCache;
/// use std::time::Duration;
///
/// let cache = Arc::new(AsyncCache::new(Duration::from_secs(10), Box::new(|| Ok(String::from("This is Sparta!")))));
/// ```
///
pub struct AsyncCache<'a, T>
where T: 'a
{
    data: RwLock<Box<dyn DataCache<Data = T> + Send + Sync + 'a>>
}

impl<'a, T> AsyncCache<'a, T>
where T: Send + Sync + 'a
{
    pub fn new(ttl: Duration, refresher: Box<dyn Fn() -> AsyncRefreshResult<T> + Send + Sync>)
        -> Self 
       {
        let datacache = AsyncDataCache {
            data: None,
            age: Instant::now() - ttl - ttl,
            ttl,
            refresher
        };
        AsyncCache {
            data: RwLock::new(Box::new(datacache) as Box<dyn DataCache<Data = T> + Send + Sync + 'a>)
        }
    }

    pub fn new_with_state<S>(
        state: S,
        ttl: Duration,
        refresher: Box<dyn Fn(&mut S) -> AsyncRefreshResult<T> + Send + Sync >
    )  -> Self 
        where S: Send + Sync + 'a
    {
        let cd  = AsyncDataCacheWithState {
            data: None,
            age: Instant::now() - ttl - ttl,
            state,
            ttl,
            refresher
        };
        AsyncCache {
            data: RwLock::new(Box::new(cd) as Box<dyn DataCache<Data = T> + Send + Sync + 'a>)
        }
    }

    pub async fn get_data(&self) -> AsyncCacheResult<T> {
        {
            let data = self.data.read().await;
            if let Ok(data) = data.get_reference() {
                return Ok(data);
            }
        }
        // Refresh if data is old
        let mut data = self.data.write().await;
        match data.refresh() {
            Ok(data) => Ok(data),
            Err(err) => Err(err)
        }
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


        let cache = Arc::new(AsyncCache::new_with_state(s, Duration::from_millis(50), Box::new(|s| {
            s.i = s.i + 1;
            Ok(s.i)
        })));
        
        let cache1 = Arc::clone(&cache);
        let t1 = tokio::spawn( async move {
            let mut data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(10)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(50)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(20)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(50)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 3);
        });

        let cache2 = Arc::clone(&cache);
        let t2 = tokio::spawn( async move {
            let mut data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(20)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(40)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(10)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(50)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 3);

        });


        t1.await.unwrap();
        t2.await.unwrap();
    }
}

