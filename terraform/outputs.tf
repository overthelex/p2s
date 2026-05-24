output "bootstrap_node" {
  value = {
    name      = docker_container.bootstrap.name
    ip        = local.bootstrap_node.ip
    backbone  = local.bootstrap_node.backbone_ip
    http_port = local.bootstrap_node.http_port
  }
}

output "region_summary" {
  value = { for region_name, region in local.regions : region_name => {
    nodes     = region.count
    subnet    = region.subnet
    delay     = region.delay
    jitter    = region.jitter
    loss      = "${region.loss}%"
    bandwidth = region.rate
    http_ports = "${region.http_base + 1}-${region.http_base + region.count}"
  }}
}

output "total_nodes" {
  value = length(local.nodes)
}
