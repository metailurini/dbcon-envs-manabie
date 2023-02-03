use colored::Colorize;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;
use std::panic;
use std::process::{Command, Stdio};
use std::str;
use std::str::from_utf8;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

static CONFIG_FILE: &'static str = "urls";
static LOCAL: &'static str = "local";
static HOST: &'static str = "localhost";
static PORT: i32 = 5432;
static SYSTEM_USERNAME: &'static str = "postgres";
static SYSTEM_PASSWORD: &'static str = "example";
static DEFAULT_DATABASE: &'static str = "bob";

#[derive(Clone)]
pub struct Environment {
    env_name: String,
    prefix_db: String,
    command_establish_connection: String,
}

macro_rules! info {
    ($($arg:tt)*) => {{
        let t = match std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH) {
                    Ok(expr) => expr.as_millis(),
                    Err(_) => return,
                };

        println!("{}", format!("[INFO-{:?}] -> {}", t, format!($($arg)*))
            .bold()
        )
    }};
}

macro_rules! warning {
    ($($arg:tt)*) => {{
        let t = match std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH) {
                    Ok(expr) => expr.as_millis(),
                    Err(_) => return,
                };

        println!("{}", format!("[WARN-{:?}] -> {}", t, format!($($arg)*))
            .yellow()
            .italic()
            .bold()
        )
    }};
}

macro_rules! error {
    ($($arg:tt)*) => {{
        let t = match std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH) {
                    Ok(expr) => expr.as_millis(),
                    Err(_) => return,
                };

        println!("{}", format!("[ERRO-{:?}] -> {}", t, format!($($arg)*))
            .red()
            .underline()
            .bold()
        )
    }};
}

fn get_envs(filename: &str) -> Result<HashMap<String, Box<Environment>>, Box<dyn Error>> {
    let config = match fs::read_to_string(filename) {
        Ok(config) => config,
        Err(err) => return Err(err.into()),
    };

    let config_lines = config
        .split('\n')
        .into_iter()
        .map(str::trim)
        .filter(|line| line.len() != 0)
        .collect::<Vec<_>>();

    let mut envs = HashMap::new();
    for line in config_lines {
        let line_details = line
            .split(";")
            .into_iter()
            .map(str::trim)
            .collect::<Vec<_>>();

        if line_details.len() < 3 {
            return Err("config file wrong format".into());
        }

        let env_name = line_details[0].to_owned();
        let prefix_db = line_details[1].to_owned();
        let command_establish_connection = line_details[2].to_owned();

        envs.insert(
            env_name.to_owned(),
            Box::new(Environment {
                env_name,
                prefix_db,
                command_establish_connection,
            }),
        );
    }

    Ok(envs)
}

fn cmd(command: String) -> std::io::Result<std::process::Output> {
    Command::new("sh").arg("-c").arg(command).output()
}

fn proc(command: String) -> std::io::Result<std::process::Output> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit())
        .output()
}

fn get_gcloud_auth_email() -> Result<String, Box<dyn Error>> {
    let output =
        match cmd("gcloud auth list | grep -o '^\\*\\s*.*@.*' | awk '{printf $2}'".to_string()) {
            Ok(output) => output.stdout,
            Err(err) => return Err(err.into()),
        };

    let email = match String::from_utf8(output) {
        Ok(email) => email,
        Err(err) => return Err(err.into()),
    };

    Ok(email)
}

fn get_postgres_pids() -> Result<Vec<String>, Box<dyn Error>> {
    let output = match cmd("lsof -i :5432".to_string()) {
        Ok(output) => output.stdout,
        Err(err) => return Err(err.into()),
    };

    let lines = match str::from_utf8(&output) {
        Ok(lines) => lines.split("\n").collect::<Vec<&str>>(),
        Err(err) => return Err(err.into()),
    };

    let mut pids = Vec::<String>::with_capacity(lines.len());
    for (index, &line) in lines.iter().enumerate() {
        if index != 0 && line.len() != 0 {
            let pid = match line.split(" ").collect::<Vec<&str>>().get(1) {
                Some(pid) => *pid,
                None => return Err("can not get pid".into()),
            };
            pids.push(pid.to_string());
        }
    }

    Ok(pids)
}

fn kill_pids(pids: Vec<String>) -> Result<(), Box<dyn Error>> {
    if pids.len() > 0 {
        let joined_pids = pids.join(" ");

        let mut kill_cmd = "kill -9 ".to_owned();
        kill_cmd.push_str(&joined_pids);

        match cmd(kill_cmd) {
            Ok(_) => {}
            Err(_) => {}
        };
    }

    Ok(())
}

fn kill_postgresql_procs() -> Result<(), Box<dyn Error>> {
    let pids = get_postgres_pids()?;
    kill_pids(pids)
}

fn start_connections(env: Environment) {
    info!("establish connection...");
    match cmd(env.command_establish_connection.to_owned()) {
        Ok(_) => {}
        Err(err) => error!("establish connection failed: {}", err),
    };
}

fn find_and_connect_psql(env: Environment, is_local: bool) {
    info!("find connections...");
    loop {
        let pids = match get_postgres_pids() {
            Ok(pids) => pids,
            Err(err) => {
                error!("get_postgresql_pids: {}", err);
                return;
            }
        };

        if pids.len() > 0 {
            info!("found a connection!");
            thread::sleep(Duration::from_secs(1));

            let user_name = match get_gcloud_auth_email() {
                Ok(email) => email,
                Err(err) => {
                    error!("get_gcloud_auth_email: {}", err);
                    return;
                }
            };
            let prefix_db = env.prefix_db.to_owned();
            let mut env_name = env.env_name.to_owned();
            let mut postgres_uri =
                format!("psql -h {HOST} -p {PORT} -U {user_name} -d {prefix_db}{DEFAULT_DATABASE}");

            if is_local {
                postgres_uri =
                    format!("psql postgres://{SYSTEM_USERNAME}:{SYSTEM_PASSWORD}@{HOST}:{PORT}/{DEFAULT_DATABASE}");
                env_name = LOCAL.to_owned();
            }

            warning!(
                "{}",
                format!("try to connect to {prefix_db}{DEFAULT_DATABASE} from {env_name}")
            );

            match proc(postgres_uri) {
                Ok(_) => {}
                Err(_) => {
                    warning!("can not establish a connection");
                    return;
                }
            };
        }
    }
}

static GLOBAL_THREAD_COUNT: AtomicUsize = AtomicUsize::new(0);

fn detach<F>(func: F)
where
    F: FnOnce() + std::marker::Send + 'static,
{
    GLOBAL_THREAD_COUNT.fetch_add(1, Ordering::SeqCst);
    thread::spawn(move || {
        func();
        GLOBAL_THREAD_COUNT.fetch_sub(1, Ordering::SeqCst);
    });
}

fn wait() {
    while GLOBAL_THREAD_COUNT.load(Ordering::SeqCst) != 0 {
        thread::sleep(Duration::from_millis(1));
    }
}

fn main() {
    match kill_postgresql_procs() {
        Ok(_) => {}
        Err(err) => {
            error!("kill_postgresql_procs: {}", err);
            return;
        }
    }

    let envs = match get_envs(CONFIG_FILE) {
        Ok(envs) => envs,
        Err(err) => {
            error!("get_envs: {}", err);
            return;
        }
    };

    let mut chosen_one = LOCAL;

    let args: Vec<_> = env::args().collect();
    if args.len() > 1 {
        chosen_one = &args[1][..];
    }
    let is_local = chosen_one == LOCAL;

    match envs.clone().get(chosen_one) {
        Some(raw_val) => {
            let start_connection_env = raw_val.as_ref().clone();
            detach(move || {
                start_connections(start_connection_env);
            });

            let find_and_connect_psql_env = raw_val.as_ref().clone();
            detach(move || {
                find_and_connect_psql(find_and_connect_psql_env, is_local);
            });

            wait();
        }
        None => panic!("choose the wrong option"),
    }
}
