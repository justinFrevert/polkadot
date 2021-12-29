// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.
#![cfg(feature = "runtime-metrics")]

const RUNTIME_METRIC_NAME: &str = "polkadot_parachain_inherent_data_bitfields_processed";
const DEFAULT_PROMETHEUS_PORT: u16 = 9615;
use std::collections::HashMap;

#[substrate_test_utils::test]
async fn runtime_can_publish_metrics() {
	use hyper::{Client, Uri};
	use polkadot_test_service::{node_config, run_validator_node, test_prometheus_config};
	use sc_client_api::{execution_extensions::ExecutionStrategies, ExecutionStrategy};
	use sp_keyring::AccountKeyring::*;
	use std::convert::TryFrom;

	let mut alice_config =
		node_config(|| {}, tokio::runtime::Handle::current(), Alice, Vec::new(), true);

	// Enable Prometheus metrics for Alice.
	alice_config.prometheus_config = Some(test_prometheus_config(DEFAULT_PROMETHEUS_PORT));

	let mut builder = sc_cli::LoggerBuilder::new("");
	// Enable profiling with wasm tracing target.
	builder.with_profiling(Default::default(), String::from("wasm_tracing=trace"));

	// Setup the runtime metrics provider.
	crate::logger_hook()(&mut builder, &alice_config);

	// Override default native strategy, we need to use the wasm runtime for this test.
	alice_config.execution_strategies = ExecutionStrategies {
		syncing: ExecutionStrategy::AlwaysWasm,
		importing: ExecutionStrategy::AlwaysWasm,
		block_construction: ExecutionStrategy::AlwaysWasm,
		offchain_worker: ExecutionStrategy::AlwaysWasm,
		other: ExecutionStrategy::AlwaysWasm,
	};

	builder.init().expect("Failed to set up the logger");

	// Start validator Alice.
	let alice = run_validator_node(alice_config, None);

	let bob_config =
		node_config(|| {}, tokio::runtime::Handle::current(), Bob, vec![alice.addr.clone()], true);

	// Start validator Bob.
	let _bob = run_validator_node(bob_config, None);

	// Wait until one block is authored by Alice.
	alice.wait_for_blocks(2).await;

	let metrics_uri = format!("http://localhost:{}/metrics", DEFAULT_PROMETHEUS_PORT);

	let res = Client::new()
		.get(Uri::try_from(&metrics_uri).expect("bad URI"))
		.await
		.expect("GET request failed");

	let body = String::from_utf8(
		hyper::body::to_bytes(res).await.expect("can't get body as bytes").to_vec(),
	)
	.expect("body is not an UTF8 string");

	let metrics = scrape_prometheus_metrics(body);

	// There should be at least 2 bitfields processed by now.
	assert!(*metrics.get(&RUNTIME_METRIC_NAME.to_owned()).unwrap() > 2);
}

fn scrape_prometheus_metrics(metrics_string: String) -> HashMap<String, u64> {
	let lines: Vec<_> = metrics_string.lines().map(|s| Ok(s.to_owned())).collect();
	prometheus_parse::Scrape::parse(lines.into_iter())
		.expect("Scraper failed to parse Prometheus metrics")
		.samples
		.into_iter()
		.map(|sample| {
			(
				sample.metric.to_owned(),
				match sample.value {
					prometheus_parse::Value::Counter(value) => value as u64,
					prometheus_parse::Value::Gauge(value) => value as u64,
					prometheus_parse::Value::Untyped(value) => value as u64,
					_ => unreachable!("unexpected metric type"),
				},
			)
		})
		.map(|(m, v)| {
			println!("{} = {}", m, v);
			(m, v)
		})
		.collect()
}
