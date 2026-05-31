---
name: launchak8scluster
description: Launch a Kubernetes cluster using Minikube with a basic application, services, and metrics logging enabled. Use this skill when you need to set up a local development Kubernetes environment with observability.
argument-hint: "[app-name] [namespace] [--disable-metrics]"
---

# Launch Kubernetes Cluster with Minikube

This skill provides a complete workflow for launching and configuring a local Kubernetes cluster using Minikube, complete with a basic application deployment, service exposure, and metrics collection.

## Prerequisites

- Minikube installed locally
- kubectl installed
- Docker or another container runtime
- Basic familiarity with Kubernetes concepts

## Step 1: Verify Minikube Installation

Check that Minikube is available and accessible:

```bash
minikube version
which minikube
```

If Minikube is not installed, provide installation instructions appropriate for the user's OS:

- **macOS**: `brew install minikube`
- **Linux**: Download from https://minikube.sigs.k8s.io/
- **Windows**: Use Chocolatey or direct download

## Step 2: Start Minikube Cluster

Launch the Minikube cluster with recommended settings for development:

```bash
minikube start \
  --driver=docker \
  --cpus=4 \
  --memory=4096 \
  --addons=metrics-server \
  --addons=ingress
```

**Options to explain:**
- `--driver=docker`: Uses Docker as the virtualization driver (use `--driver=hyperkit` on macOS Intel or `--driver=qemu` for ARM64)
- `--cpus=4`: Allocate 4 CPUs to the cluster
- `--memory=4096`: Allocate 4GB RAM to the cluster
- `--addons=metrics-server`: Enable metrics collection for resource monitoring
- `--addons=ingress`: Enable ingress controller for external traffic

Wait for Minikube to fully start (30-60 seconds typically).

## Step 3: Verify Cluster Status

Confirm the cluster is running and all nodes are ready:

```bash
kubectl cluster-info
kubectl get nodes
kubectl get nodes -o wide
```

Expected output: At least one node with status "Ready"

## Step 4: Create Application Namespace

Create an isolated namespace for the application (recommended for organization):

```bash
kubectl create namespace app-ns
```

Or use the default namespace. Set the context:

```bash
kubectl config set-context --current --namespace=app-ns
```

## Step 5: Deploy Basic Application

Create a simple sample application (e.g., a Nginx web server or basic Node.js app).

### Option A: Deploy Nginx (quickest)

```bash
kubectl create deployment web-app --image=nginx:latest --replicas=3
```

### Option B: Deploy Custom Application

If the user has a containerized application, deploy it:

```bash
kubectl create deployment web-app --image=your-registry/your-app:latest --replicas=2
```

## Step 6: Expose Application via Service

Create a Kubernetes Service to expose the application:

```bash
kubectl expose deployment web-app --type=LoadBalancer --port=80 --target-port=80
```

For Minikube, use `LoadBalancer` type (Minikube handles this locally). In another terminal, tunnel the service:

```bash
minikube tunnel
```

This makes the LoadBalancer service accessible at `localhost`.

## Step 7: Verify Deployment

Check that pods are running and the service is exposed:

```bash
kubectl get pods
kubectl get svc
kubectl describe svc web-app
```

Get the service URL:

```bash
minikube service web-app --url
# Or with tunnel running:
curl http://localhost
```

## Step 8: Enable Metrics Logging

Metrics-server is already added as an addon in Step 2. Verify it's running:

```bash
kubectl get deployment metrics-server -n kube-system
```

### Check Metrics

Wait 1-2 minutes for metrics to start collecting, then query node and pod metrics:

```bash
# Node metrics
kubectl top nodes

# Pod metrics
kubectl top pods -n app-ns

# Pod metrics with resource requests/limits
kubectl describe pod -n app-ns <pod-name>
```

### Enable Verbose Logging (Optional)

For detailed application logs:

```bash
kubectl logs -f deployment/web-app -n app-ns
```

For kubelet logs (cluster-level):

```bash
minikube logs --follow
```

## Step 9: Set Up Port Forwarding (Alternative to Tunnel)

If you prefer direct pod access instead of a service tunnel:

```bash
kubectl port-forward svc/web-app 8080:80 -n app-ns
```

Then access the app at `http://localhost:8080`

## Step 10: Verify Complete Setup

Run a final verification:

```bash
# Check cluster status
kubectl cluster-info dump --output-directory=./cluster-dump

# Verify metrics collection
kubectl get --all-namespaces pods | grep metrics-server

# Test application connectivity
curl http://localhost  # If using tunnel
# or
curl http://localhost:8080  # If using port-forward
```

## Cleanup

When finished, clean up resources:

```bash
# Delete the deployment and service
kubectl delete deployment web-app -n app-ns
kubectl delete svc web-app -n app-ns

# Delete the namespace
kubectl delete namespace app-ns

# Stop Minikube (keeps cluster data)
minikube stop

# Full cleanup (removes cluster data)
minikube delete
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `minikube: command not found` | Install Minikube or add to PATH |
| `kubectl: connection refused` | Run `minikube start` first |
| `LoadBalancer pending` | Use `minikube tunnel` in a separate terminal |
| `No metrics available` | Wait 2-3 minutes for metrics-server to collect data; check `kubectl get deployment metrics-server -n kube-system` |
| `Pod stuck in Pending` | Check node resources: `kubectl describe nodes` |
| `Can't connect to deployed app` | Verify service is running: `kubectl get svc`, then use `minikube tunnel` or `port-forward` |

## Common Commands Reference

```bash
# View everything
kubectl get all -n app-ns

# Tail application logs
kubectl logs -f deployment/web-app -n app-ns

# Describe deployment for debugging
kubectl describe deployment web-app -n app-ns

# Edit deployment (e.g., change replicas)
kubectl edit deployment web-app -n app-ns

# Scale deployment
kubectl scale deployment web-app --replicas=5 -n app-ns

# Get service details
kubectl get service web-app -n app-ns -o yaml

# Check resource usage
kubectl top pods -n app-ns
kubectl top nodes
```

## Next Steps

- **Persistent storage**: Add PersistentVolumes and PersistentVolumeClaims
- **Ingress**: Configure ingress rules for more advanced routing
- **ConfigMaps & Secrets**: Manage app configuration and credentials
- **Health checks**: Add liveness and readiness probes
- **Monitoring**: Install Prometheus or Grafana for advanced metrics
- **CI/CD**: Integrate with local registry to test deployment pipelines

