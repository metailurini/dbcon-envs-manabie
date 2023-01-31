use colored::Colorize;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::panic;
use std::process::{Command, Stdio};
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

static HOST: &'static str = "localhost";
static PORT: i32 = 5432;
static SYSTEM_USERNAME: &'static str = "postgres";
static SYSTEM_PASSWORD: &'static str = "example";
static DEFAULT_DATABASE: &'static str = "bob";

static LOCAL: &'static str = "local";

static CONFIG: &'static str = "
    local          ;        ; kubectl -n emulator port-forward service/postgres-infras 5432:5432
    stag.cmn       ;        ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:manabie-common-88e1ee71=tcp:5432
    stag.lms       ;        ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:manabie-lms-de12e08e=tcp:5432
    stag.jprep     ; stag_  ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:jprep-uat=tcp:5432

    uat.cmn        ; uat_   ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:manabie-common-88e1ee71=tcp:5432
    uat.lms        ; uat_   ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:manabie-lms-de12e08e=tcp:5432
    uat.jprep      ;        ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:jprep-uat=tcp:5432

    prod.aic       ; aic_   ; cloud_sql_proxy -enable_iam_login -instances=student-coach-e1e95:asia-northeast1:jp-partners-b04fbb69=tcp:5432
    prod.ga        ; ga_    ; cloud_sql_proxy -enable_iam_login -instances=student-coach-e1e95:asia-northeast1:jp-partners-b04fbb69=tcp:5432
    prod.jprep     ;        ; cloud_sql_proxy -enable_iam_login -instances=live-manabie:asia-northeast1:jprep-6a98=tcp:5432
    prod.renseikai ;        ; cloud_sql_proxy -enable_iam_login -instances=production-renseikai:asia-northeast1:renseikai-83fc=tcp:5432
    prod.synersia  ;        ; cloud_sql_proxy -enable_iam_login -instances=synersia:asia-northeast1:synersia-228d=tcp:5432
    prod.tokyo     ; tokyo_ ; cloud_sql_proxy -enable_iam_login -instances=student-coach-e1e95:asia-northeast1:prod-tokyo=tcp:5432
";

#[derive(Copy, Clone)]
pub struct Environment {
    env_name: &'static str,
    prefix_db: &'static str,
    command_establish_connection: &'static str,
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

fn get_envs() -> HashMap<String, Box<Environment>> {
    let mut envs = HashMap::new();
    _ = CONFIG
        .split("\n")
        .map(|line| line.trim())
        .filter(|line| line.len() != 0)
        .map(|line| {
            let line_details = line.split(";").map(|x| x.trim()).collect::<Vec<&str>>();

            let environment_name = match line_details.get(0) {
                Some(en) => *en,
                _none => "",
            };

            let prefix_db = match line_details.get(1) {
                Some(pdb) => *pdb,
                _none => "",
            };

            let command_establish_connection = match line_details.get(2) {
                Some(cec) => *cec,
                _none => "",
            };

            envs.insert(
                environment_name.to_owned(),
                Box::new(Environment {
                    env_name: environment_name,
                    prefix_db,
                    command_establish_connection,
                }),
            );

            line
        })
        .collect::<Vec<&str>>();
    envs
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

    let email = match str::from_utf8(&output) {
        Ok(email) => String::from(email),
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
                _none => return Err("can not get pid".into()),
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
            let prefix_db = env.prefix_db;
            let mut env_name = env.env_name;
            let mut postgres_uri =
                format!("psql -h {HOST} -p {PORT} -U {user_name} -d {prefix_db}{DEFAULT_DATABASE}");

            if is_local {
                postgres_uri =
                    format!("psql postgres://{SYSTEM_USERNAME}:{SYSTEM_PASSWORD}@{HOST}:{PORT}/{DEFAULT_DATABASE}");
                env_name = LOCAL;
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

    let envs = get_envs();

    let mut chosen_one = LOCAL;

    let args: Vec<_> = env::args().collect();
    if args.len() > 1 {
        chosen_one = &args[1][..];
    }
    let is_local = chosen_one == LOCAL;

    match envs.get(chosen_one) {
        Some(raw_val) => {
            let env = raw_val.as_ref().clone();

            detach(move || {
                start_connections(env);
            });

            detach(move || {
                find_and_connect_psql(env, is_local);
            });

            wait();
        }
        None => panic!("choose the wrong option"),
    }
}
