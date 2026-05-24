terraform {
  required_providers {
    docker = {
      source  = "kreuzwerker/docker"
      version = "~> 3.0"
    }
  }
}

provider "docker" {
  host = "unix:///var/run/docker.sock"
}

# ═══════════════════════════════════════════════════════════
# 200-node P2S testnet across 5 simulated "regions"
#
# Region layout:
#   datacenter  — 60 nodes — fast, reliable (intra-DC)
#   broadband   — 50 nodes — typical ISP connection
#   emerging    — 40 nodes — developing-world links
#   mobile      — 30 nodes — cellular with jitter
#   satellite   — 20 nodes — high latency, lossy
#
# tc/netem profiles applied inside each container.
# All nodes connected to backbone + regional network.
# ═══════════════════════════════════════════════════════════

locals {
  image_name = "docker-p2s-node:latest"

  regions = {
    datacenter = {
      count   = 60
      subnet  = "10.60.0.0/16"
      gateway = "10.60.0.1"
      ip_base = "10.60.1"
      delay   = "1ms"
      jitter  = "0.5ms"
      loss    = "0"
      rate    = "1000mbit"
      http_base = 20000
    }
    broadband = {
      count   = 50
      subnet  = "10.61.0.0/16"
      gateway = "10.61.0.1"
      ip_base = "10.61.1"
      delay   = "25ms"
      jitter  = "5ms"
      loss    = "0.1"
      rate    = "100mbit"
      http_base = 21000
    }
    emerging = {
      count   = 40
      subnet  = "10.62.0.0/16"
      gateway = "10.62.0.1"
      ip_base = "10.62.1"
      delay   = "80ms"
      jitter  = "20ms"
      loss    = "1"
      rate    = "10mbit"
      http_base = 22000
    }
    mobile = {
      count   = 30
      subnet  = "10.63.0.0/16"
      gateway = "10.63.0.1"
      ip_base = "10.63.1"
      delay   = "120ms"
      jitter  = "40ms"
      loss    = "2"
      rate    = "5mbit"
      http_base = 23000
    }
    satellite = {
      count   = 20
      subnet  = "10.64.0.0/16"
      gateway = "10.64.0.1"
      ip_base = "10.64.1"
      delay   = "550ms"
      jitter  = "50ms"
      loss    = "3"
      rate    = "2mbit"
      http_base = 24000
    }
  }

  backbone_subnet  = "10.50.0.0/16"
  backbone_gateway = "10.50.0.1"

  nodes = flatten([
    for region_name, region in local.regions : [
      for i in range(region.count) : {
        name        = "p2s-${region_name}-${i + 1}"
        region      = region_name
        index       = i
        ip          = "${region.ip_base}.${(i + 10) % 256}"
        backbone_ip = "10.50.${index(keys(local.regions), region_name) + 1}.${i + 10}"
        http_port   = region.http_base + i + 1
        delay       = region.delay
        jitter      = region.jitter
        loss        = region.loss
        rate        = region.rate
        network     = region_name
      }
    ]
  ])

  bootstrap_node = local.nodes[0]
}

# ═══ Networks ═══
resource "docker_network" "backbone" {
  name   = "p2s-backbone"
  driver = "bridge"
  ipam_config {
    subnet  = local.backbone_subnet
    gateway = local.backbone_gateway
  }
}

resource "docker_network" "region" {
  for_each = local.regions
  name     = "p2s-${each.key}"
  driver   = "bridge"
  ipam_config {
    subnet  = each.value.subnet
    gateway = each.value.gateway
  }
}

# ═══ Bootstrap node ═══
resource "docker_container" "bootstrap" {
  name  = local.bootstrap_node.name
  image = local.image_name

  networks_advanced {
    name         = docker_network.region["datacenter"].id
    ipv4_address = local.bootstrap_node.ip
  }
  networks_advanced {
    name         = docker_network.backbone.id
    ipv4_address = local.bootstrap_node.backbone_ip
  }

  ports {
    internal = 8080
    external = local.bootstrap_node.http_port
    ip       = "127.0.0.1"
  }

  command = [
    "--listen", "/ip4/0.0.0.0/tcp/4001",
    "--http-port", "8080",
    "--data-dir", "/data",
  ]

  env     = ["RUST_LOG=p2s_node=info,libp2p=warn"]
  restart = "unless-stopped"

  capabilities { add = ["NET_ADMIN"] }

  upload {
    content = templatefile("${path.module}/netem.sh", {
      delay  = local.bootstrap_node.delay
      jitter = local.bootstrap_node.jitter
      loss   = local.bootstrap_node.loss
      rate   = local.bootstrap_node.rate
    })
    file       = "/tmp/netem.sh"
    executable = true
  }

  healthcheck {
    test     = ["CMD", "wget", "-q", "--spider", "http://localhost:8080/health"]
    interval = "10s"
    timeout  = "5s"
    retries  = 5
  }

  memory = 128
  lifecycle { ignore_changes = [image] }
}

# ═══ Remaining 199 nodes ═══
resource "docker_container" "node" {
  for_each = { for node in slice(local.nodes, 1, length(local.nodes)) : node.name => node }

  name  = each.value.name
  image = local.image_name

  networks_advanced {
    name         = docker_network.region[each.value.network].id
    ipv4_address = each.value.ip
  }
  networks_advanced {
    name         = docker_network.backbone.id
    ipv4_address = each.value.backbone_ip
  }

  ports {
    internal = 8080
    external = each.value.http_port
    ip       = "127.0.0.1"
  }

  command = [
    "--listen", "/ip4/0.0.0.0/tcp/4001",
    "--http-port", "8080",
    "--data-dir", "/data",
    "--bootstrap-peer", "/ip4/${local.bootstrap_node.backbone_ip}/tcp/4001/p2p/${var.bootstrap_peer_id}",
  ]

  env     = ["RUST_LOG=p2s_node=info,libp2p=warn"]
  restart = "unless-stopped"

  capabilities { add = ["NET_ADMIN"] }

  upload {
    content = templatefile("${path.module}/netem.sh", {
      delay  = each.value.delay
      jitter = each.value.jitter
      loss   = each.value.loss
      rate   = each.value.rate
    })
    file       = "/tmp/netem.sh"
    executable = true
  }

  healthcheck {
    test     = ["CMD", "wget", "-q", "--spider", "http://localhost:8080/health"]
    interval = "10s"
    timeout  = "5s"
    retries  = 5
  }

  memory     = 128
  depends_on = [docker_container.bootstrap]
  lifecycle  { ignore_changes = [image] }
}
