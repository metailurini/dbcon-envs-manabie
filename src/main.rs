use colored::Colorize;
use std::collections::HashMap;
use std::error::Error;
use std::panic;
use std::process::{Command, Stdio};
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

static CONFIG: &'static str = "
    stag.manabie   ;        ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:manabie-59fd=tcp:5432
    stag.jprep     ; stag_  ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:jprep-uat=tcp:5432

    uat.manabie    ; uat_   ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:manabie-59fd=tcp:5432
    uat.jprep      ;        ; cloud_sql_proxy -enable_iam_login -instances=staging-manabie-online:asia-southeast1:jprep-uat=tcp:5432

    prod.aic       ; aic_   ; cloud_sql_proxy -enable_iam_login -instances=student-coach-e1e95:asia-northeast1:jp-partners-b04fbb69=tcp:5432
    prod.ga        ; ga_    ; cloud_sql_proxy -enable_iam_login -instances=student-coach-e1e95:asia-northeast1:jp-partners-b04fbb69=tcp:5432
    prod.jprep     ;        ; cloud_sql_proxy -enable_iam_login -instances=live-manabie:asia-northeast1:jprep-6a98=tcp:5432
    prod.renseikai ;        ; cloud_sql_proxy -enable_iam_login -instances=production-renseikai:asia-northeast1:renseikai-83fc=tcp:5432
    prod.synersia  ;        ; cloud_sql_proxy -enable_iam_login -instances=synersia:asia-northeast1:synersia-228d=tcp:5432
    prod.tokyo     ; tokyo_ ; cloud_sql_proxy -enable_iam_login -instances=student-coach-e1e95:asia-northeast1:prod-tokyo=tcp:5432
";

#[derive(Copy, Clone)]
pub struct Enviroment {
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

fn get_envs() -> HashMap<String, Box<Enviroment>> {
    let mut envs = HashMap::new();
    _ = CONFIG
        .split("\n")
        .map(|line| line.trim())
        .filter(|line| line.len() != 0)
        .map(|line| {
            let line_details = line.split(";").map(|x| x.trim()).collect::<Vec<&str>>();

            let enviroment_name = match line_details.get(0) {
                Some(en) => *en,
                None => "",
            };

            let prefix_db = match line_details.get(1) {
                Some(pdb) => *pdb,
                None => "",
            };

            let command_establish_connection = match line_details.get(2) {
                Some(cec) => *cec,
                None => "",
            };

            envs.insert(
                enviroment_name.to_owned(),
                Box::new(Enviroment {
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

fn start_connections(env: Enviroment) {
    info!("establish connection...");
    match cmd(env.command_establish_connection.to_owned()) {
        Ok(_) => {}
        Err(err) => error!("establish connection failed: {}", err),
    };
}

fn find_connections(env: Enviroment) {
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
            warning!("found a connection!");
            break;
        }
    }

    let prefix_db = env.prefix_db;
    match proc(format!(
        "psql -h localhost -p 5432 -U thanhdanh.nguyen@manabie.com -d {prefix_db}bob"
    )) {
        Ok(_) => {}
        Err(err) => {
            error!("psql: {}", err);
            return;
        }
    };
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
    let choosen_one = "uat.jprep";

    match envs.get(choosen_one) {
        Some(raw_val) => {
            let env = raw_val.as_ref().clone();

            detach(move || {
                start_connections(env);
            });

            detach(move || {
                find_connections(env);
            });

            wait();
        }
        None => panic!("choose the wrong option"),
    }
}
