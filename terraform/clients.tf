# ═══════════════════════════════════════════════════════════
# Client simulators — 2-10 clients per node publishing cards
#
# Each client is a lightweight Alpine container running a
# bash loop that POSTs cards to its assigned node.
#
# Client distribution:
#   datacenter nodes: 5-10 clients (high load)
#   broadband nodes:  3-7 clients (medium load)
#   emerging nodes:   2-5 clients
#   mobile nodes:     2-4 clients
#   satellite nodes:  2-3 clients (low load)
#
# Total clients: ~1000 (200 nodes × ~5 avg)
# ═══════════════════════════════════════════════════════════

locals {
  client_ranges = {
    datacenter = { min = 5, max = 10 }
    broadband  = { min = 3, max = 7 }
    emerging   = { min = 2, max = 5 }
    mobile     = { min = 2, max = 4 }
    satellite  = { min = 2, max = 3 }
  }

  # Generate client assignments per node
  # Use a deterministic "random" based on node index
  clients = flatten([
    for node in local.nodes : [
      for c in range(
        local.client_ranges[node.region].min + (node.index % (local.client_ranges[node.region].max - local.client_ranges[node.region].min + 1))
      ) : {
        name        = "${node.name}-client-${c + 1}"
        node_name   = node.name
        node_ip     = node.ip
        node_region = node.region
        network     = node.network
        client_id   = "${node.region}-n${node.index + 1}-c${c + 1}"
        card_count  = c + 1  # varied load per client
        interval    = node.region == "datacenter" ? 5 : node.region == "broadband" ? 10 : 15
      }
    ]
  ])
}

resource "docker_image" "client" {
  name = "p2s-client:latest"
  build {
    context    = "${path.module}/client"
    dockerfile = "Dockerfile"
  }
}

resource "docker_container" "client" {
  for_each = { for client in local.clients : client.name => client }

  name  = each.value.name
  image = docker_image.client.image_id

  networks_advanced {
    name = docker_network.region[each.value.node_region].id
  }

  env = [
    "NODE_URL=http://${each.value.node_ip}:8080",
    "CLIENT_ID=${each.value.client_id}",
    "CARD_COUNT=${each.value.card_count}",
    "INTERVAL=${each.value.interval}",
  ]

  restart = "unless-stopped"
  memory  = 32

  depends_on = [docker_container.bootstrap]

  lifecycle {
    ignore_changes = [image]
  }
}

output "client_summary" {
  value = { for region_name, range in local.client_ranges : region_name => {
    min_per_node = range.min
    max_per_node = range.max
    total_clients = length([for c in local.clients : c if c.node_region == region_name])
  }}
}

output "total_clients" {
  value = length(local.clients)
}
