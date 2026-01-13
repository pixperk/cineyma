use cineyma::{Actor, Context, Handler, Message};
use std::collections::HashMap;

use crate::proto::{
    DeleteRequest, DeleteResponse, GetRequest, GetResponse, SetRequest, SetResponse,
};

///simple in-memory key-value store actor
pub struct KVStore {
    data: HashMap<String, String>,
}

impl KVStore {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

impl Actor for KVStore {}

//message trait implementations
impl Message for GetRequest {
    type Result = GetResponse;
}
impl cineyma::remote::RemoteMessage for GetRequest {}

impl Message for GetResponse {
    type Result = ();
}
impl cineyma::remote::RemoteMessage for GetResponse {}

impl Message for SetRequest {
    type Result = SetResponse;
}
impl cineyma::remote::RemoteMessage for SetRequest {}

impl Message for SetResponse {
    type Result = ();
}
impl cineyma::remote::RemoteMessage for SetResponse {}

impl Message for DeleteRequest {
    type Result = DeleteResponse;
}
impl cineyma::remote::RemoteMessage for DeleteRequest {}

impl Message for DeleteResponse {
    type Result = ();
}
impl cineyma::remote::RemoteMessage for DeleteResponse {}

//handler implementations
impl Handler<GetRequest> for KVStore {
    fn handle(&mut self, msg: GetRequest, _ctx: &mut Context<Self>) -> GetResponse {
        let value = self.data.get(&msg.key).cloned();
        println!("[KVStore] GET {} -> {:?}", msg.key, value);
        GetResponse { value }
    }
}

impl Handler<SetRequest> for KVStore {
    fn handle(&mut self, msg: SetRequest, _ctx: &mut Context<Self>) -> SetResponse {
        self.data.insert(msg.key.clone(), msg.value.clone());
        println!("[KVStore] SET {} = {}", msg.key, msg.value);
        SetResponse { success: true }
    }
}

impl Handler<DeleteRequest> for KVStore {
    fn handle(&mut self, msg: DeleteRequest, _ctx: &mut Context<Self>) -> DeleteResponse {
        let deleted = self.data.remove(&msg.key).is_some();
        println!("[KVStore] DELETE {} -> {}", msg.key, deleted);
        DeleteResponse { deleted }
    }
}
