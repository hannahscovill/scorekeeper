terraform {
  required_version = ">= 1.0"

  required_providers {
    grafana = {
      source  = "grafana/grafana"
      version = "~> 3.0"
    }
  }

  # Store state in S3 - update bucket/key for your environment
  # backend "s3" {
  #   bucket = "your-terraform-state-bucket"
  #   key    = "grafana/terraform.tfstate"
  #   region = "us-west-2"
  # }
}

provider "grafana" {
  url  = var.grafana_url
  auth = var.grafana_api_key
}

variable "grafana_url" {
  description = "Grafana Cloud instance URL (e.g. https://your-org.grafana.net)"
  type        = string
}

variable "grafana_api_key" {
  description = "Grafana API key with Editor permissions"
  type        = string
  sensitive   = true
}

variable "tempo_datasource_uid" {
  description = "UID of the Tempo data source in Grafana (find in Connections > Data sources > Tempo)"
  type        = string
  default     = "grafanacloud-traces"
}

# Folder to organize dashboards
resource "grafana_folder" "scorekeeper" {
  title = "Scorekeeper"
}

# Provision all dashboard JSON files from the dashboards/ directory
resource "grafana_dashboard" "dashboards" {
  for_each = fileset("${path.module}/dashboards", "*.json")

  folder = grafana_folder.scorekeeper.id
  config_json = templatefile(
    "${path.module}/dashboards/${each.value}",
    {
      tempo_datasource_uid = var.tempo_datasource_uid
    }
  )
  overwrite = true
}
