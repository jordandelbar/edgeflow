variable "TAG" {
  default = "dev"
}

group "default" {
  targets = ["server", "inference-tract", "inference-ort"]
}

target "server" {
  dockerfile = "deploy/server.Dockerfile"
  context    = "."
  tags       = ["edgeflow-server:${TAG}"]
}

target "inference-tract" {
  dockerfile = "deploy/inference.Dockerfile"
  context    = "."
  args       = { BACKEND = "tract-backend" }
  tags       = ["edgeflow-inference:${TAG}-tract"]
}

target "inference-ort" {
  dockerfile = "deploy/inference.Dockerfile"
  context    = "."
  args       = { BACKEND = "ort-backend" }
  tags       = ["edgeflow-inference:${TAG}-ort"]
}
