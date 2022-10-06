use clap::ArgMatches;
use std::error::Error;

use crate::https::{HttpsClient, ClientBuilder};
//use crate::error::Error as RestError;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct State {
    pub client: HttpsClient,
}

impl State {
    pub async fn new(opts: ArgMatches) -> BoxResult<Self> {
        // Set timeout
        let timeout: u64 = opts
            .value_of("timeout")
            .unwrap()
            .parse()
            .unwrap_or_else(|_| {
                eprintln!("Supplied timeout not in range, defaulting to 60");
                60
            });

        let client = ClientBuilder::new().timeout(timeout).build()?;

        Ok(State {
            client,
        })
    }
}
