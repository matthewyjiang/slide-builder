use super::*;

fn client_with_env(enabled: &str, socket: Option<&str>, pane: Option<&str>) -> HerdrClient {
    HerdrClient::from_env_vars(|key| match key {
        "HERDR_ENV" => Some(enabled.to_owned()),
        "HERDR_SOCKET_PATH" => socket.map(str::to_owned),
        "HERDR_PANE_ID" => pane.map(str::to_owned),
        _ => None,
    })
}

#[tokio::test]
async fn incomplete_or_disabled_environment_is_inert() {
    for client in [
        client_with_env("0", Some("socket"), Some("pane")),
        client_with_env("true", Some("socket"), Some("pane")),
        client_with_env("1", None, Some("pane")),
        client_with_env("1", Some(""), Some("pane")),
        client_with_env("1", Some("socket"), None),
        client_with_env("1", Some("socket"), Some("")),
    ] {
        assert_eq!(
            client.graphics_capability().await,
            HerdrGraphicsCapability::NotHerdr
        );
        let reporter = client.start_reporting("session");
        reporter.report(HerdrState::Working, None);
        reporter.shutdown().await.unwrap();
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::path::Path;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
        sync::mpsc,
    };

    fn client_for_socket(path: &Path) -> HerdrClient {
        client_with_env("1", path.to_str(), Some("w1:p1"))
    }

    struct Server {
        requests: mpsc::UnboundedReceiver<Value>,
        replies: mpsc::UnboundedSender<Vec<u8>>,
        task: JoinHandle<()>,
    }

    impl Server {
        fn bind(path: &Path) -> Self {
            let listener = UnixListener::bind(path).unwrap();
            let (requests_tx, requests) = mpsc::unbounded_channel();
            let (replies, mut replies_rx) = mpsc::unbounded_channel::<Vec<u8>>();
            let task = tokio::spawn(async move {
                let mut _previous_stream = None;
                while let Ok((stream, _)) = listener.accept().await {
                    let mut stream = BufReader::new(stream);
                    let mut line = String::new();
                    stream.read_line(&mut line).await.unwrap();
                    requests_tx
                        .send(serde_json::from_str(&line).unwrap())
                        .unwrap();
                    let Some(reply) = replies_rx.recv().await else {
                        break;
                    };
                    stream.get_mut().write_all(&reply).await.unwrap();
                    // Keep the response connection open until the next request.
                    // The client must use newline framing, not wait for EOF.
                    _previous_stream = Some(stream);
                }
            });
            Self {
                requests,
                replies,
                task,
            }
        }

        async fn next(&mut self, reply: &[u8]) -> Value {
            let request = self.requests.recv().await.unwrap();
            self.replies.send(reply.to_vec()).unwrap();
            request
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    #[test]
    fn reporting_without_runtime_does_not_panic() {
        let reporter = client_for_socket(Path::new("missing.socket")).start_reporting("session");
        reporter.report(HerdrState::Idle, None);
        assert!(reporter.last_error().unwrap().contains("Tokio runtime"));
    }

    #[tokio::test]
    async fn reports_session_and_states_before_release() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let reporter = client_for_socket(&socket).start_reporting("session-123");
        for (state, text) in [
            (HerdrState::Working, "working"),
            (HerdrState::Blocked, "blocked"),
            (HerdrState::Idle, "idle"),
        ] {
            reporter.report(state, Some("status"));
            let report = server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await;
            let mut expected = json!({
                "pane_id": "w1:p1", "source": "herdr:slide-builder",
                "agent": "slide-builder", "agent_session_id": "session-123",
            });
            expected["state"] = json!(text);
            expected["message"] = json!("status");
            assert_eq!(report["method"], "pane.report_agent");
            assert_eq!(report["params"], expected);
        }
        // Closing the sender must not discard its unconsumed final update.
        reporter.report(HerdrState::Idle, None);
        let shutdown = tokio::spawn(reporter.shutdown());
        let last = server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await;
        assert_eq!(last["method"], "pane.report_agent");
        assert!(last["params"].get("message").is_none());
        let release = server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await;
        assert_eq!(release["method"], "pane.release_agent");
        assert_eq!(
            release["params"],
            json!({"pane_id": "w1:p1", "source": "herdr:slide-builder", "agent": "slide-builder"})
        );
        shutdown.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn graphics_probes_validate_host_cells() {
        for (reply, expected) in [
            (
                "{\"result\":{\"cell_width_px\":9,\"cell_height_px\":18},\"error\":null}\n",
                HerdrGraphicsCapability::Paintable {
                    width: 9,
                    height: 18,
                },
            ),
            (
                "{\"result\":{\"cell_width_px\":0,\"cell_height_px\":18}}\n",
                HerdrGraphicsCapability::Unpaintable,
            ),
            (
                "{\"result\":{\"cell_width_px\":65536,\"cell_height_px\":18}}\n",
                HerdrGraphicsCapability::Unpaintable,
            ),
            (
                "{\"result\":{\"cell_width_px\":9}}\n",
                HerdrGraphicsCapability::Unpaintable,
            ),
            (
                "{\"error\":{\"message\":\"unavailable\"}}\n",
                HerdrGraphicsCapability::Unpaintable,
            ),
            ("not-json\n", HerdrGraphicsCapability::Unpaintable),
            ("{}\n", HerdrGraphicsCapability::Unpaintable),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let socket = temp.path().join("herdr.sock");
            let mut server = Server::bind(&socket);
            let client = client_for_socket(&socket);
            let probe = tokio::spawn(async move { client.graphics_capability().await });
            let request = server.next(reply.as_bytes()).await;
            assert_eq!(request["method"], "pane.graphics.info");
            assert_eq!(request["params"], json!({"pane_id": "w1:p1"}));
            assert_eq!(probe.await.unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn missing_socket_is_nonfatal() {
        let temp = tempfile::tempdir().unwrap();
        let client = client_for_socket(&temp.path().join("missing.sock"));
        assert_eq!(
            client.graphics_capability().await,
            HerdrGraphicsCapability::Unpaintable
        );
        let reporter = client.start_reporting("session");
        reporter.report(HerdrState::Working, None);
        assert!(reporter.shutdown().await.is_err());
    }

    #[tokio::test]
    async fn pending_transitions_coalesce_and_retry_after_failure() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let reporter = client_for_socket(&socket).start_reporting("session");
        reporter.report(HerdrState::Working, None);
        let first = server.requests.recv().await.unwrap();
        // The worker is waiting for the first response. Only the latest update
        // should remain pending, regardless of the number of reports.
        for state in [HerdrState::Idle, HerdrState::Blocked, HerdrState::Working] {
            reporter.report(state, None);
        }
        server.replies.send(b"malformed\n".to_vec()).unwrap();
        let retry = server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await;
        assert_eq!(first["params"], retry["params"]);
        assert!(reporter
            .last_error()
            .unwrap()
            .contains("malformed response"));
        let shutdown = tokio::spawn(reporter.shutdown());
        assert_eq!(
            server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await["method"],
            "pane.release_agent"
        );
        assert!(shutdown
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("malformed response"));
    }

    #[tokio::test]
    async fn drop_still_releases_without_waiting() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let reporter = client_for_socket(&socket).start_reporting("session");
        reporter.report(HerdrState::Working, None);
        server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await;
        drop(reporter);
        assert_eq!(
            server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await["method"],
            "pane.release_agent"
        );
    }

    #[tokio::test]
    async fn successful_identical_reports_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let reporter = client_for_socket(&socket).start_reporting("session");
        reporter.report(HerdrState::Working, Some("building"));
        let first = server.requests.recv().await.unwrap();
        assert_eq!(first["method"], "pane.report_agent");
        reporter.report(HerdrState::Working, Some("building"));
        server
            .replies
            .send(b"{\"result\":{\"type\":\"ok\"}}\n".to_vec())
            .unwrap();
        let shutdown = tokio::spawn(reporter.shutdown());
        assert_eq!(
            server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await["method"],
            "pane.release_agent"
        );
        shutdown.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn stalled_graphics_probe_is_unpaintable() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let client = client_for_socket(&socket);
        let probe = tokio::spawn(async move { client.graphics_capability().await });
        server.requests.recv().await.unwrap();
        // Withhold the reply to exercise the production graphics time budget.
        assert_eq!(probe.await.unwrap(), HerdrGraphicsCapability::Unpaintable);
    }

    #[tokio::test]
    async fn rpc_failure_recovers_on_next_transition() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let reporter = client_for_socket(&socket).start_reporting("session");
        reporter.report(HerdrState::Working, None);
        assert_eq!(
            server.next(b"{\"error\":\"try again\"}\n").await["method"],
            "pane.report_agent"
        );
        reporter.report(HerdrState::Idle, None);
        assert_eq!(
            server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await["method"],
            "pane.report_agent"
        );
        let shutdown = tokio::spawn(reporter.shutdown());
        server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await;
        assert!(shutdown
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("try again"));
    }

    #[tokio::test]
    async fn failed_identical_reports_do_not_retry_on_ui_ticks() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let reporter = client_for_socket(&socket).start_reporting("session");
        reporter.report(HerdrState::Working, None);
        server.next(b"{\"error\":\"unavailable\"}\n").await;
        // Yield until the worker records the failure, then mimic another UI tick.
        while reporter.last_error().is_none() {
            tokio::task::yield_now().await;
        }
        reporter.report(HerdrState::Working, None);
        let shutdown = tokio::spawn(reporter.shutdown());
        assert_eq!(
            server.next(b"{\"result\":{\"type\":\"ok\"}}\n").await["method"],
            "pane.release_agent"
        );
        assert!(shutdown.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn response_overflow_names_budget_limit_and_received_size() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("herdr.sock");
        let mut server = Server::bind(&socket);
        let config = client_for_socket(&socket).config.unwrap();
        let request =
            tokio::spawn(
                async move { exchange(&config, "test", json!({}), REQUEST_TIMEOUT).await },
            );
        server
            .next(&vec![b' '; MAX_RESPONSE_BYTES as usize + 1])
            .await;
        let error = request.await.unwrap().unwrap_err().to_string();
        assert!(error.contains("response byte budget"));
        assert!(error.contains("limit 65536"));
        assert!(error.contains("received at least 65537"));
    }
}
