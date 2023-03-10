/*
 * Copyright [2022,2023] <mcr@sandelman.ca>

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
pub mod acp_interface;
pub mod join_interface;
pub mod args;
pub mod grasp;
pub mod graspsamples;
pub mod error;

use crate::args::RoosterOptions;
use crate::interfaces::AllInterfaces;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {

    // construct a subscriber that prints formatted traces to stdout
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // process the arguments to find out which interface is the uplink
    // interface.
    let mainargs = RoosterOptions::from_args();
    println!("Read in args: {:?}\n", mainargs);

    let mut debug_options = crate::debugoptions::DebugOptions::default();
    if mainargs.debug_interfaces {
        debug_options.debug_interfaces = true;
    }
    if mainargs.debug_registrars {
        debug_options.debug_registrars = true;
    }
    if mainargs.debug_joininterfaces {
        debug_options.debug_joininterfaces = true;
    }
    if mainargs.debug_proxyactions {
        debug_options.debug_proxyactions   = true;
    }
    println!("Debug Interfaces is {}", debug_options.debug_interfaces);

    let mut binterface = AllInterfaces::default();
    let debug = Arc::new(debug_options);
    binterface.debug = debug.clone();
    let interface = Arc::new(Mutex::new(binterface));

    let listeninterface = interface.clone();
    let listenfuture  = AllInterfaces::listen_network(&listeninterface,
                                                      &mainargs);
    listenfuture.await.unwrap();

    let mut main_loopcount = 0;

    let mut done = false;
    while !done {
        sleep(Duration::from_millis(5000)).await;

        main_loopcount += 1;
        debug.debug_info(format!("{} main loop", main_loopcount)).await;

        done = {

            let (lji_hash, proxies, isitdone) = {
                let mut doneinterface = interface.lock().await;

                doneinterface.update_available_registrars().await;
                (doneinterface.joinlink_interfaces.clone(), doneinterface.proxies.clone(), doneinterface.exitnow)
            };
            // doneinterface lock is released here.
            let ji_hash = lji_hash.lock().await;

            for ljl in ji_hash.values() {
                let jl = ljl.lock().await;
                let session_id = rand::random::<u32>();
                jl.registrar_all_announce(proxies.clone(),
                                          session_id).await.unwrap();
            }

            isitdone
        };
        debug.debug_info(format!("{} end main loop", main_loopcount)).await;
    }
}


/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo build"
 * End:
 */
