variable "TAG" {
  default = "dev"
}

group "default" {
  targets = ["server", "inference"]
}

target "server" {
  dockerfile = "deploy/server.Dockerfile"
  context    = "."
  tags       = ["edgeflow-server:${TAG}"]
}

target "inference" {
  dockerfile = "deploy/inference.Dockerfile"
  context    = "."
  tags       = ["edgeflow-inference:${TAG}"]
}
