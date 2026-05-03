# Changelog

## [0.4.1](https://github.com/jordandelbar/edgeflow/compare/v0.4.0...v0.4.1) (2026-05-03)


### Bug Fixes

* Bump manylinux to 2_28 and add wheel smoke ([#34](https://github.com/jordandelbar/edgeflow/issues/34)) ([298c5d7](https://github.com/jordandelbar/edgeflow/commit/298c5d72411b03fcce3b7f64d39c8074a3c319c9))

## [0.4.0](https://github.com/jordandelbar/edgeflow/compare/v0.3.0...v0.4.0) (2026-05-03)


### ⚠ BREAKING CHANGES

* edgeflow.deploy() now returns a Deployment dataclass instead of a dict. EDGEFLOW_SERVER env var or explicit server= kwarg is now required: the silent localhost fallback was removed because it hid configuration mistakes.

### Features

* Bring Python SDK to parity with the CLI ([#28](https://github.com/jordandelbar/edgeflow/issues/28)) ([0c6dbae](https://github.com/jordandelbar/edgeflow/commit/0c6dbaefc37f0a1cbe2b090bd859d74d48f8e69b))
* Make CORS opt-in via env var ([#18](https://github.com/jordandelbar/edgeflow/issues/18)) ([c9a19c5](https://github.com/jordandelbar/edgeflow/commit/c9a19c5dedd0ab605072f401bee73e08dbe2d01f))
* Schema-driven inputs for named and image targets ([#25](https://github.com/jordandelbar/edgeflow/issues/25)) ([1ae7035](https://github.com/jordandelbar/edgeflow/commit/1ae703540d085e6262c007b1217500a190fe2982))


### Bug Fixes

* Bump cli edgeflow-client dep alongside workspace version ([#31](https://github.com/jordandelbar/edgeflow/issues/31)) ([84e40ce](https://github.com/jordandelbar/edgeflow/commit/84e40ce3bae9e2e76cb8d08ea6bfefe6e0d33771))


### Performance

* Pool wire buffers and drop legacy WASM path ([#10](https://github.com/jordandelbar/edgeflow/issues/10)) ([ea06ed8](https://github.com/jordandelbar/edgeflow/commit/ea06ed847cc995b331ef27e994665d843a4e3f21))


### Refactor

* K8s reflector ([#14](https://github.com/jordandelbar/edgeflow/issues/14)) ([3deadf2](https://github.com/jordandelbar/edgeflow/commit/3deadf268c3fdfec398274e8c067b9902d5d32fc))
* Shared inference session ([#24](https://github.com/jordandelbar/edgeflow/issues/24)) ([dbf89d9](https://github.com/jordandelbar/edgeflow/commit/dbf89d905634cc313b40df207aaa2bdd88225cd2))

## [0.3.0](https://github.com/jordandelbar/edgeflow/compare/v0.2.1...v0.3.0) (2026-04-25)


### ⚠ BREAKING CHANGES

* Replace pre-transform double-instantiation with transform-from ([#5](https://github.com/jordandelbar/edgeflow/issues/5))

### Bug Fixes

* Add boosting extras for xgboost and lightgbm onnx export ([#8](https://github.com/jordandelbar/edgeflow/issues/8)) ([eda4239](https://github.com/jordandelbar/edgeflow/commit/eda42391e9ff397429edca4552d57c8700b0f5cd))
* Compose first deploys stuck in pending state ([#7](https://github.com/jordandelbar/edgeflow/issues/7)) ([073e431](https://github.com/jordandelbar/edgeflow/commit/073e4319932e9ceff4e1d8517aa442c8bba41b72))
* Run preprocess pre-transforms on JSON array inputs ([9f50b3f](https://github.com/jordandelbar/edgeflow/commit/9f50b3f3377df129c3185af286a9ebcd20968d5e))


### Refactor

* Replace pre-transform double-instantiation with transform-from ([#5](https://github.com/jordandelbar/edgeflow/issues/5)) ([e7bddfa](https://github.com/jordandelbar/edgeflow/commit/e7bddfaaa0123aabe4321e92bf21125897a10474))

## [0.2.1](https://github.com/jordandelbar/edgeflow/compare/v0.2.0...v0.2.1) (2026-04-25)


### Bug Fixes

* Trigger v0.2.1 release ([74a5d72](https://github.com/jordandelbar/edgeflow/commit/74a5d721c71241935cc1295b8d9adb2ddcb1bcbb))

## [0.2.0](https://github.com/jordandelbar/edgeflow/compare/v0.1.1...v0.2.0) (2026-04-25)


### Features

* First public release ([edca206](https://github.com/jordandelbar/edgeflow/commit/edca206b75648b64b2ef050cfd53810c483c301a))


### Bug Fixes

* Use simple release-type for cargo workspace ([37f8530](https://github.com/jordandelbar/edgeflow/commit/37f85300fced411f24c4f488237825dde8d89a91))
