//! `rediss://` against a TLS-only Redis (docker/redis, profile `tls`):
//!   docker/redis/gen-certs.sh
//!   docker compose -f docker/redis/docker-compose.yml --profile tls up -d --wait
//!   TIQ_REDIS_TLS_URL=rediss://127.0.0.1:56381/0 SSL_CERT_FILE=docker/redis/tls/ca.crt \
//!     cargo test --test redis_tls
//!
//! The server's certificate is signed by a throwaway CA that only `SSL_CERT_FILE` trusts, and it
//! names 127.0.0.1 only: the connection must be verified, not merely encrypted. Port 56382 is a
//! server whose certificate (same CA) names something else.

mod common;
use common::live_url;

use tableiq_lib::drivers::kv_redis::RedisDriver;
use tableiq_lib::drivers::{KvDriver, KvReadOpts};

#[tokio::test]
async fn tls_connections_are_verified() {
    let Some(url) = live_url("TIQ_REDIS_TLS_URL") else { return };
    assert!(std::env::var_os("SSL_CERT_FILE").is_some(), "SSL_CERT_FILE must name docker/redis/tls/ca.crt");

    // Trusted CA, matching name: connects and reads.
    let drv = RedisDriver::connect(&url).await.unwrap();
    let r = drv.read("tls:1", &KvReadOpts::default()).await.unwrap();
    assert_eq!(r.items[0].value.as_ref().unwrap().text, "secure");

    // The same CA's certificate for another name (the `redis-tls-wrong` server): refused, and
    // for that reason — not a timeout.
    let wrong = url.replace("56381", "56382");
    let err = RedisDriver::connect(&wrong).await.err().expect("a name the certificate does not carry").to_string();
    assert!(err.to_lowercase().contains("certificate"), "{err}");

    // Plain Redis on a TLS port is not spoken to either.
    let plain = url.replacen("rediss://", "redis://", 1);
    assert!(RedisDriver::connect(&plain).await.is_err());
}
