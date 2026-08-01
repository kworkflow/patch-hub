use super::{HttpMethod, MockNetClientTrait, NetClientTrait, NetError};

#[test]
fn mock_net_client_returns_configured_response() {
    let mut mock = MockNetClientTrait::new();
    mock.expect_request()
        .returning(|_, _| Ok("<html>response</html>".to_string()));

    let result = mock.request(HttpMethod::Get, "https://example.com");
    assert!(result.is_ok());
    assert_eq!("<html>response</html>", result.unwrap());
}

#[test]
fn mock_net_client_can_return_http_status_error() {
    let mut mock = MockNetClientTrait::new();
    mock.expect_request().returning(|_, _| {
        Err(NetError::HttpStatus {
            code: 404,
            message: "HTTP 404".to_string(),
        })
    });

    let result = mock.request(HttpMethod::Get, "https://example.com/missing");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("404"));
}

#[test]
#[ignore = "network-io"]
fn ureq_net_client_can_reach_lore_kernel_org() {
    use super::UreqNetClient;
    let client = UreqNetClient::new();
    let result = client.request(HttpMethod::Get, "https://lore.kernel.org/?&o=0");
    assert!(result.is_ok(), "Expected successful response: {result:?}");
}
