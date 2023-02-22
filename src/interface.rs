/*
 * Copyright [2022, 2023] <mcr@sandelman.ca>

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

// this module listens for interfaces and then sorts them into three lists
// according to the provided list of acp interfaces, joinlink interfaces, and
// interfacs to ignore.
// The lists may include glob(1) wildcards.
//
// Interfaces which match none of the lists are placed into the joinlink interface
// list if the list is empty, otherwise, they are ignored
//

use std::net::Ipv6Addr;

use netlink_packet_route::link::nlas::State;
use std::sync::Arc;
use futures::lock::Mutex;

use crate::debugoptions::DebugOptions;
use crate::args::RoosterOptions;
use crate::acp_interface::AcpInterface;
use crate::join_interface::JoinInterface;

pub type IfIndex = u32;

pub struct Interface {
    pub debug:         Arc<DebugOptions>,
    pub ifindex:       IfIndex,
    pub ifname:        String,
    pub ignored:       bool,
    pub mtu:           u32,
    pub linklocal6:    Ipv6Addr,
    pub oper_state:    State,
    pub acp_daemon:    Option<Arc<Mutex<AcpInterface>>>,
    pub join_daemon:   Option<Arc<Mutex<JoinInterface>>>
}

impl Interface {
    pub fn default(debug: Arc<DebugOptions>) -> Interface {
        Interface {
            debug:   debug.clone(),
            ifindex: 0,
            ifname:  "".to_string(),
            ignored: false,
            mtu:     0,
            linklocal6: Ipv6Addr::UNSPECIFIED,
            oper_state: State::Down,
            acp_daemon: None,
            join_daemon: None,
        }
    }
    pub fn empty(ifi: IfIndex, debug: Arc<DebugOptions>) -> Interface {
        let mut d = Self::default(debug);
        d.ifindex = ifi;
        d
    }

    pub async fn start_acp(self: &mut Self,
                           _options: &RoosterOptions,
                           mydebug: Arc<DebugOptions>,
                           invalidate: Arc<Mutex<bool>>) {

        mydebug.debug_info(format!("starting Registrar listener on ACP interface {}", self.ifname)).await;
        self.acp_daemon = Some(AcpInterface::start_daemon(&self, invalidate.clone()).await.unwrap());
    }

    pub async fn calculate_available_registrar(self: &Interface) -> (bool, bool, bool) {
        if let Some(lacp) = &self.acp_daemon {
            let acp = lacp.lock().await;
            return acp.calculate_available_registrar().await;
        } else {
            return (false, false, false);
        }
    }


    pub async fn start_joinlink(self: &mut Self,
                                _options: &RoosterOptions,
                                mydebug: Arc<DebugOptions>,
                                invalidate: Arc<Mutex<bool>>) {

        mydebug.debug_info(format!("starting JoinProxy announcer on joinlink interface {}", self.ifname)).await;
        self.join_daemon = Some(
            JoinInterface::start_daemon(&self, invalidate.clone()).await.unwrap());
    }
}


#[cfg(test)]
pub mod tests {
    use crate::interfaces::AllInterfaces;
    use super::*;

    #[allow(unused_macros)]
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    async fn async_start_acp(_ifn: &mut Interface) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn setup_ai() -> (Arc<Mutex<Vec<u8>>>, AllInterfaces) {
        let writer: Vec<u8> = vec![];
        let awriter = Arc::new(Mutex::new(writer));
        let db1 = DebugOptions { debug_interfaces: true,
                                 verydebug_interfaces: false,
                                 debug_output: awriter.clone() };
        let mut all1 = AllInterfaces::default();
        all1.debug = Arc::new(db1);

        (awriter, all1)
    }

    #[test]
    fn test_start_acp() -> Result<(), std::io::Error> {
        let (_awriter, all1) = setup_ai();
        let mut ifn = Interface::empty(1, all1.debug);
        aw!(async_start_acp(&mut ifn)).unwrap();
        Ok(())
    }



}

/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo test"
 * End:
 */
