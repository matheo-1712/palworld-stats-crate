use palworld_stats::{Error, PlatformId, PlayerUid, RestClient};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn players_maps_fields_and_drops_ip() {
    let server = MockServer::start().await;
    let body = serde_json::json!({"players":[
        {"name":"Alice","accountName":"alice_acc","playerId":"3c36ec75000000000000000000000000",
         "userId":"steam_76561198000000001","ip":"203.0.113.77","ping":12.5,
         "location_x":100.5,"location_y":-200.0,"level":12,"building_count":3},
        {"name":"Bob","accountName":"bob_acc","playerId":"aabbccdd000000000000000000000000",
         "userId":"xbox_2535400000000000","ip":"198.51.100.9","ping":30,
         "location_x":1,"location_y":2,"level":12.0,"building_count":0}
    ]});
    Mock::given(method("GET"))
        .and(path("/v1/api/players"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let client = RestClient::new(format!("{}/", server.uri()), "secret").unwrap();
    let players = client.players().await.unwrap();
    assert_eq!(players.len(), 2);
    assert_eq!(
        players[0].uid,
        PlayerUid::parse("3C36EC75000000000000000000000000").unwrap()
    );
    assert_eq!(
        players[0].platform_id,
        PlatformId::Steam("76561198000000001".into())
    );
    assert_eq!(players[0].level, Some(12));
    assert_eq!(players[0].location_x, Some(100.5));
    assert_eq!(players[0].building_count, Some(3));
    assert_eq!(
        players[1].platform_id,
        PlatformId::Xbox("2535400000000000".into())
    );
    assert_eq!(players[1].level, Some(12));
    assert_eq!(players[1].ping, Some(30.0));
    let json = serde_json::to_string(&players).unwrap();
    assert!(!json.contains("203.0.113.77"));
    assert!(!json.contains("198.51.100.9"));
}

#[tokio::test]
async fn basic_auth_is_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/api/info"))
        .and(header("Authorization", "Basic YWRtaW46c2VjcmV0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version":"v0.6.0","servername":"S","description":"D","worldguid":"ABC"
        })))
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "secret").unwrap();
    let info = client.info().await.unwrap();
    assert_eq!(info.server_name, "S");
    assert_eq!(info.world_guid, "ABC");
    assert_eq!(info.version, "v0.6.0");
}

#[tokio::test]
async fn unauthorized_maps_to_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "bad").unwrap();
    assert!(matches!(client.players().await, Err(Error::Unauthorized)));
}

#[tokio::test]
async fn other_status_maps_to_status_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "x").unwrap();
    assert!(matches!(client.metrics().await, Err(Error::Status(500))));
}

#[tokio::test]
async fn malformed_body_maps_to_unexpected_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "x").unwrap();
    assert!(matches!(
        client.metrics().await,
        Err(Error::UnexpectedResponse(_))
    ));
}

#[tokio::test]
async fn nil_player_id_is_dropped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/api/players"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"players":[
                {"name":"Loading","accountName":"l","playerId":"00000000000000000000000000000000","userId":"steam_1"},
                {"name":"Ok","accountName":"o","playerId":"01","userId":"steam_2"}
            ]})),
        )
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "x").unwrap();
    let players = client.players().await.unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(players[0].name, "Ok");
}

#[tokio::test]
async fn empty_players_is_empty_vec() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/api/players"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"players":[]})))
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "x").unwrap();
    assert!(client.players().await.unwrap().is_empty());
}

#[tokio::test]
async fn bad_player_id_is_dropped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/api/players"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"players":[
                {"name":"Bad","accountName":"b","playerId":"zzzz","userId":"steam_1"},
                {"name":"Ok","accountName":"o","playerId":"01","userId":"steam_2"}
            ]})),
        )
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "x").unwrap();
    let players = client.players().await.unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(players[0].name, "Ok");
    assert_eq!(players[0].level, None);
}

#[tokio::test]
async fn metrics_without_new_fields() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/api/metrics"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "serverfps":60,"currentplayernum":3,"serverframetime":16.6,
            "maxplayernum":32,"uptime":1234
        })))
        .mount(&server)
        .await;
    let client = RestClient::new(server.uri(), "x").unwrap();
    let m = client.metrics().await.unwrap();
    assert_eq!(m.server_fps, Some(60.0));
    assert_eq!(m.current_players, 3);
    assert_eq!(m.max_players, 32);
    assert_eq!(m.uptime_seconds, 1234);
    assert_eq!(m.days, None);
    assert_eq!(m.basecamp_count, None);
}

#[tokio::test]
async fn server_down_is_http_error() {
    let client = RestClient::new("http://127.0.0.1:1", "x").unwrap();
    assert!(matches!(client.info().await, Err(Error::Http(_))));
}

#[test]
fn debug_does_not_leak_password() {
    let c = RestClient::new("http://x", "secret").unwrap();
    assert!(!format!("{c:?}").contains("secret"));
}
