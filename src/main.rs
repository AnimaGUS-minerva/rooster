/*
 * Copyright [2022] <mcr@sandelman.ca>

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
 *
 */

//use tokio::runtime;
use structopt::StructOpt;
use std::sync::Arc;
use futures::lock::Mutex;
use tokio::time::{sleep, Duration};

pub mod debugoptions;
pub mod interfaces;
pub mod interface;
pub mod args;

use crate::args::RoosterOptions;
use crate::interfaces::AllInterfaces;

#[tokio::main]
async fn main() {

    // process the arguments to find out which interface is the uplink
    // interface.
    let mainargs = RoosterOptions::from_args();
    println!("Read in args: {:?}\n", mainargs);

    let mut debug_options = crate::debugoptions::DebugOptions::default();
    if mainargs.debug_graspmessages {
        debug_options.debug_interfaces = true;
    }

    let mut binterface = AllInterfaces::default();
    binterface.debug = debug_options;
    let interface = Arc::new(Mutex::new(binterface));

    let listeninterface = interface.clone();
    let listenfuture  = AllInterfaces::listen_network(&listeninterface,
                                                      &mainargs);
    listenfuture.await.unwrap();
    sleep(Duration::from_millis(1000000)).await;
}


/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo build"
 * End:
 */
