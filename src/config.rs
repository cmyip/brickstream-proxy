use rocket::config::Environment;
use rocket::Config;

pub fn from_env() -> Config {
    let environment = Environment::active().expect("No environment found");
    Config::build(environment)
        .environment(environment)
        .port(3000)
        .finalize()
        .unwrap()
}
