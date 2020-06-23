use std::fs;
use std::collections::HashMap;

use serde::{Serialize, Deserialize};

use crate::internal_api::InternalApi;
use crate::internal_api::ApiMessage::{User, Logout};

impl DynamicBot
{
    //Constructor returns LoggedOutDBot with registered user list
    pub fn new(config_path: String) -> LoggedOutDBot
    {
        let yaml_string: String= fs::read_to_string(&config_path)
            .expect(&format!("Couldn't open {}", &config_path));
        let dbot: LoggedOutDBot = serde_yaml::from_str(&yaml_string).unwrap();
        dbot
    }

    //Listens for users to login from api and returns music
    pub fn listen_respond(&self) 
    {
        loop
        {
            match self.api.recieve_response()
            {
                User(name) => {
                    println!("{}", name);
                    let music = self.get_music(name);
                    println!("{}", music);
                    self.api.send_music(music);
                },
                Logout() => break
            }
        }
    }

    //Logs out Dbot
    pub fn logout(self) -> LoggedOutDBot
    {
        let bot = LoggedOutDBot{users: self.users, token: self.api.close()};
        bot
    }
}

impl LoggedOutDBot
{
    //Login function returns usable dbot 
    pub fn login(self) -> DynamicBot
    {
        let api = InternalApi::new(self.token);
        let bot = DynamicBot{users: self.users, api: api};
        bot
    }
}

//DynamicBot stores HashMap of users and their music and api
pub struct DynamicBot
{
    users: HashMap<String, String>,
    api: InternalApi,
}

//LoggedOutDBot stores hashmap of registered users and config data.
#[derive(Serialize, Deserialize)]
pub struct LoggedOutDBot
{
    users: HashMap<String, String>,
    token: String, 
}

//Private function accesses internal hashmap along with default value
impl DynamicBot
{
    fn get_music(&self, user: String) -> String
    {
        match self.users.get(&user)
        {
            Some(music) => return music.to_string(),
            _ => return "https://www.youtube.com/watch?v=J87xZuMdrKQ".to_string(),
        }
    }
}
