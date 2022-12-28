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

// this module listens for interfaces and then sorts them into three lists
// according to the provided list of acp interfaces, downlink interfaces, and
// interfacs to ignore.
// The lists may include glob(1) wildcards.
//
// Interfaces which match none of the lists are placed into the downlink interface
// list if the list is empty, otherwise, they are ignored
//

use std::net::Ipv6Addr;

use netlink_packet_route::link::nlas::State;
use std::sync::Arc;
use futures::lock::Mutex;

use crate::debugoptions::DebugOptions;
use crate::args::RoosterOptions;
use crate::acp_interface::AcpInterface;

pub type IfIndex = u32;

pub enum InterfaceType {
    Ignored,
    AcpUpLink { acp_daemon: Arc<Mutex<AcpInterface>> },
    JoinDownLink
}

pub struct Interface {
    pub ifindex:       IfIndex,
    pub ifname:        String,
    pub ignored:       bool,
    pub mtu:           u32,
    pub linklocal6:    Ipv6Addr,
    pub oper_state:    State,
    pub daemon:        InterfaceType
}

impl Interface {
    pub fn default() -> Interface {
        Interface {
            ifindex: 0,
            ifname:  "".to_string(),
            ignored: false,
            mtu:     0,
            linklocal6: Ipv6Addr::UNSPECIFIED,
            oper_state: State::Down,
            daemon: InterfaceType::Ignored
        }
    }
    pub fn empty(ifi: IfIndex) -> Interface {
        let mut d = Self::default();
        d.ifindex = ifi;
        d
    }

    pub async fn start_acp(self: &mut Self, _options: &RoosterOptions, mut mydebug: DebugOptions) {

        mydebug.debug_info(format!("starting Registrar listener on ACP interface {}", self.ifname)).await;
        self.daemon = InterfaceType::AcpUpLink {
            acp_daemon: AcpInterface::start_daemon(&self).await.unwrap()
        };
    }

    pub async fn start_joinlink(self: &Self, _options: &RoosterOptions, mut mydebug: DebugOptions) {

        mydebug.debug_info(format!("starting JoinProxy announcer on downlink interface {}", self.ifname)).await;
    }
}


#[cfg(test)]
pub mod tests {
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

    #[test]
    fn test_start_acp() -> Result<(), std::io::Error> {
        //let (_awriter, mut all1) = setup_ai();
        let mut ifn = Interface::empty(1);
        aw!(async_start_acp(&mut ifn)).unwrap();
        Ok(())
    }



}

/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && RUSTFLAGS='-A dead_code -Awarnings' cargo build"
 * End:
 */
