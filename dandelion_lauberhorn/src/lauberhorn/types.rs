use crate::{lauberhorn::marshal::{}, 
webserver::schemas::{InputSet}};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DandelionArgs {
    pub sets: Vec<InputSet>
}

// User sends XDR encoded request with {function_name, blob}
// where blob is the bjson-encoded dandelion request context
