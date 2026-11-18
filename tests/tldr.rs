use std::fs;
use std::process::{Command, Stdio};

use mockito::Server;
use serde_json::Value;

mod support;
use support::{openai_stream, request_query};

const TAR: &str = "# tar\n\n> Archiving utility.\n> More information: <https://www.gnu.org/software/tar>.\n\n- Create an archive from files:\n\n`tar cf {{path/to/target.tar}} {{path/to/file1 path/to/file2 ...}}`\n\n- E[x]tract an archive in a target directory:\n\n`tar xf {{path/to/source.tar}} {{[-C|--directory]}} {{path/to/directory}}`\n";
const GIT_COMMIT: &str = "# git commit\n\n> Commit files to the repository.\n\n- Replace the last commit with currently staged changes:\n\n`git commit --amend`\n";
const ALIAS: &str = "# docker run\n\n> This command is an alias of `docker container run`.\n\n- View documentation for the original command:\n\n`tldr docker container run`\n";
const ORIGINAL: &str = "# docker container run\n\n> Run a command in a new container.\n\n- Run a container in the background:\n\n`docker {{[run|container run]}} {{[-d|--detach]}} {{image}}`\n";

fn pages() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let common = dir.path().join("pages/common");
    fs::create_dir_all(&common).unwrap();
    fs::write(common.join("tar.md"), TAR).unwrap();
    fs::write(common.join("git-commit.md"), GIT_COMMIT).unwrap();
    fs::write(common.join("docker-run.md"), ALIAS).unwrap();
    fs::write(common.join("docker-container-run.md"), ORIGINAL).unwrap();
    dir
}

fn ruask(pages: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruask"));
    command
        .env_clear()
        .env("RUASK_TLDR_DIR", pages)
        .args(["-L", "en", "-p", "linux", "--color", "never"])
        .stdin(Stdio::null());
    command
}

#[test]
fn bare_command_with_a_page_is_answered_locally_without_model_configuration() {
    let dir = pages();
    let output = ruask(dir.path()).arg("tar").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Archiving utility."));
    assert!(!stdout.contains("More information"));
    assert!(stdout.contains("    tar xf {{path/to/source.tar}} -C {{path/to/directory}}"));
}

#[test]
fn alias_pages_follow_the_original_command() {
    let dir = pages();
    let output = ruask(dir.path()).args(["docker", "run"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("docker container run -d {{image}}"),
        "{stdout}"
    );
}

#[test]
fn question_sends_the_command_page_as_reference_to_the_model() {
    let dir = pages();
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            let query: Value = request_query(request);
            query["question"] == "fix the last commit message"
                && query["reference"]
                    .as_str()
                    .is_some_and(|r| r.contains("`git commit --amend`"))
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("- Amend:\n`git commit --amend`"))
        .create();
    let output = ruask(dir.path())
        .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model")
        .args(["-q", "fix the last commit message", "git", "commit"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    mock.assert();
}

#[test]
fn unknown_commands_and_missing_pages_keep_the_model_path() {
    let dir = pages();
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| request_query(request)["reference"].is_null())
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("- Run it:\n`mytool --help`"))
        .create();
    let output = ruask(dir.path())
        .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model")
        .arg("mytool")
        .output()
        .unwrap();
    assert!(output.status.success());
    mock.assert();
}

const SS: &str =
    "# ss\n\n> Utility to investigate sockets.\n\n- Show all TCP sockets:\n\n`ss -t`\n";
const KUBECTL: &str = "# kubectl\n\n> Command-line interface for Kubernetes clusters.\n\n- List all pods:\n\n`kubectl get pods`\n";
const KUBECTL_LOGS: &str = "# kubectl logs\n\n> Show logs for containers in a pod.\n\n- Show logs for a single-container pod:\n\n`kubectl logs {{pod_name}}`\n";
const KUBECTL_APPLY: &str = "# kubectl apply\n\n> Manage applications through files defining Kubernetes resources.\n\n- Apply a configuration to a resource:\n\n`kubectl apply -f {{path/to/file}}`\n";
const RM: &str = "# rm\n\n> Remove files or directories.\n\n- Remove files:\n\n`rm {{path/to/file}}`\n\n- Remove directories recursively:\n\n`rm {{[-r|--recursive]}} {{path/to/directory}}`\n";

fn more_pages() -> tempfile::TempDir {
    let dir = pages();
    let linux = dir.path().join("pages/linux");
    let common = dir.path().join("pages/common");
    fs::create_dir_all(&linux).unwrap();
    fs::write(linux.join("ss.md"), SS).unwrap();
    fs::write(common.join("kubectl.md"), KUBECTL).unwrap();
    fs::write(common.join("kubectl-logs.md"), KUBECTL_LOGS).unwrap();
    fs::write(common.join("kubectl-apply.md"), KUBECTL_APPLY).unwrap();
    fs::write(common.join("rm.md"), RM).unwrap();
    dir
}

fn with_model(command: &mut Command, server: &Server) {
    command
        .env("OPENAI_BASE_URL", format!("{}/v1", server.url()))
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_MODEL", "test-model");
}

#[test]
fn pages_for_another_platform_are_marked_and_attributed() {
    let dir = more_pages();
    let output = ruask(dir.path())
        .args(["-p", "osx", "ss"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("this page is for Linux only"), "{stdout}");
    assert!(
        stdout.trim_end().ends_with("tldr-pages · CC BY 4.0"),
        "{stdout}"
    );
    let same = ruask(dir.path())
        .args(["-p", "linux", "ss"])
        .output()
        .unwrap();
    assert!(!String::from_utf8(same.stdout).unwrap().contains("only"));
}

#[test]
fn a_single_close_typo_is_corrected_locally_and_reported_on_stderr() {
    let dir = more_pages();
    let output = ruask(dir.path()).args(["kubetcl"]).output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Kubernetes")
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("kubectl")
    );
}

#[test]
fn recursive_deletes_are_flagged_under_the_command() {
    let dir = more_pages();
    let output = ruask(dir.path()).arg("rm").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("    rm -r {{path/to/directory}}\n    ⚠ Destructive"),
        "{stdout}"
    );
    assert_eq!(stdout.matches("⚠").count(), 1, "{stdout}");
}

#[test]
fn chinese_questions_pick_family_pages_through_english_terms_and_list_the_rest() {
    let dir = more_pages();
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_request(|request| {
            let reference = request_query(request)["reference"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            reference.contains("## kubectl-logs\n")
                && !reference.contains("## kubectl-apply\n")
                && reference.contains("## other kubectl pages: kubectl-apply")
        })
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream("- Logs:\n`kubectl logs {{pod}}`"))
        .create();
    let mut command = ruask(dir.path());
    with_model(&mut command, &server);
    let output = command
        .args(["-L", "zh", "-q", "查看 pod 日志", "kubectl"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    mock.assert();
}

#[test]
fn environment_is_sent_only_when_the_target_is_this_host() {
    let dir = more_pages();
    let bin = tempfile::tempdir().unwrap();
    let brew = bin.path().join("brew");
    fs::write(&brew, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&brew, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let host = if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };
    let other = if host == "osx" { "linux" } else { "osx" };
    for (platform, expect) in [(host, true), (other, false)] {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_request(move |request| {
                let environment = &request_query(request)["environment"];
                if expect {
                    environment["shell"] == "zsh"
                        && environment["package_managers"] == serde_json::json!(["brew"])
                        && environment["queried_command_installed"] == false
                } else {
                    environment.is_null()
                }
            })
            .with_header("content-type", "text/event-stream")
            .with_body(openai_stream("> ok"))
            .create();
        let mut command = ruask(dir.path());
        with_model(&mut command, &server);
        let output = command
            .env("PATH", bin.path())
            .env("SHELL", "/bin/zsh")
            .args(["-p", platform, "-q", "how to list pods", "kubectl"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        mock.assert();
    }
}

#[test]
fn update_pages_installs_english_and_chinese_pages_with_an_index() {
    let work = tempfile::tempdir().unwrap();
    let archive = work.path().join("tldr.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        ("LICENSE.md", "CC BY 4.0"),
        ("pages/common/tar.md", TAR),
        ("pages/common/kubectl-logs.md", KUBECTL_LOGS),
        (
            "pages.zh/common/tar.md",
            "# tar\n\n> 归档工具。\n\n- 创建归档：\n\n`tar cf {{a.tar}} {{文件}}`\n",
        ),
        ("pages.fr/common/tar.md", "# tar\n"),
        ("../escape.md", "nope"),
    ] {
        zip.start_file(name, options).unwrap();
        std::io::Write::write_all(&mut zip, body.as_bytes()).unwrap();
    }
    for i in 0..10 {
        zip.start_file(format!("pages/linux/p{i}.md"), options)
            .unwrap();
        std::io::Write::write_all(&mut zip, b"# p\n").unwrap();
    }
    zip.finish().unwrap();
    let target = work.path().join("data/tldr");
    let output = Command::new(env!("CARGO_BIN_EXE_ruask"))
        .env_clear()
        .env("RUASK_TLDR_DIR", &target)
        .args(["--update-pages", "--from"])
        .arg(&archive)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("CC BY 4.0")
    );
    assert!(target.join("pages.zh/common/tar.md").is_file());
    assert!(!target.join("pages.fr").exists());
    assert!(!work.path().join("escape.md").exists());
    let index = fs::read_to_string(target.join("ruask-index.tsv")).unwrap();
    assert!(index.starts_with("#ruask-index v1\n"));
    assert!(index.contains("en\tcommon\tkubectl-logs\tShow logs for containers in a pod."));
    let local = ruask(&target).args(["-L", "zh", "tar"]).output().unwrap();
    assert!(
        String::from_utf8(local.stdout)
            .unwrap()
            .contains("归档工具")
    );
}

#[test]
fn update_pages_must_be_used_alone() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruask"))
        .env_clear()
        .args(["--update-pages", "tar"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--update-pages")
    );
}

#[test]
fn a_purpose_written_as_an_explanation_stays_attached_to_its_command() {
    let dir = more_pages();
    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_header("content-type", "text/event-stream")
        .with_body(openai_stream(
            "> Use -L.\n> Forward a port:\n`ssh -L 8080:localhost:80 host`",
        ))
        .create();
    let mut command = ruask(dir.path());
    with_model(&mut command, &server);
    let output = command
        .args(["-q", "forward a port", "ssh"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("  Forward a port:\n    ssh -L 8080:localhost:80 host"),
        "{stdout}"
    );
}
