# flagrant Helm chart

Deploys `flagrant-api` on Kubernetes. SQLite backs the app's storage, so this
chart is built around two constraints that don't apply to a typical stateless
service:

- **Single writer.** Only one `flagrant-api` process may hold the database at
  a time. `replicaCount` is fixed at `1`, and the Deployment uses
  `strategy: Recreate` so the old pod is fully gone before the new one
  starts. Don't attach an HPA to this chart.
- **No durable local disk.** The SQLite file lives on an `emptyDir`, not a
  PersistentVolume. Durability comes from [Litestream](https://litestream.io/)
  continuously replicating the write-ahead log to an S3 bucket. A
  `litestream-restore` init container fetches the latest snapshot from S3
  before `flagrant-api` ever starts (this matters because
  `flagrant`'s `init_pool()` calls `create_if_missing(true)` - without the
  restore step, a missing file wouldn't fail loudly, it would silently start
  fresh and empty).

See `templates/NOTES.txt` (printed on install/upgrade) for the operational
caveats this implies, especially around shutdown ordering.

## Required values

`image` defaults to `docker.io/mbuczko/flagrant-api:latest`. At minimum you
must also set the Litestream S3 replica target:

```yaml
litestream:
  replica:
    bucket: your-s3-bucket
    region: eu-west-1                  # or set litestream.replica.endpoint for non-AWS S3
```

## Secrets

`flagrant.toml` (mounted as `FLAGRANT_CONFIG`) can contain per-environment
`srv-token`s - see the repo's root `flagrant.toml` for the format. This chart
always renders it into a Kubernetes `Secret`, never a ConfigMap. Two ways to
supply it:

- `config.content`: inline TOML, rendered into a Secret owned by this
  release. Convenient, but avoid committing real tokens this way into a
  values file checked into git.
- `config.existingSecret` / `config.existingSecretKey`: point at a Secret
  managed outside Helm (e.g. by External Secrets Operator, Sealed Secrets, or
  `kubectl create secret` directly). Takes precedence over `config.content`.

Litestream's S3 credentials follow the same pattern (`litestream.s3.*`). If
you leave both `existingSecret` and the inline `accessKeyId`/`secretAccessKey`
empty, no credential environment variables are injected at all - use this on
EKS with `serviceAccount.create: true` and an IRSA role annotation under
`serviceAccount.annotations` instead of static credentials.

## Health checks

Readiness/liveness probes are plain HTTP GETs against `/version`, which
returns the running `flagrant-api` crate version and touches nothing else
(no DB, no Redis) - keep `service.port` in sync with the `[http] listen`
port set in `config.content`.

## Out of scope

Redis-backed response caching and the gRPC feature-resolution endpoint are
both optional `flagrant-api` features enabled purely via `[redis]`/`[grpc]`
sections in `flagrant.toml` - this chart doesn't deploy Redis itself. Point
`[redis] url` at an instance you manage separately if you want caching.

## Installing the published chart

Every tagged release publishes this chart to the same Gitea instance as the
Docker image and FreeBSD package (see `.gitea/workflows/dockerize.yml`,
`publish-helm-chart` job), as a Helm chart repository
(https://docs.gitea.com/usage/packages/helm):

```sh
helm repo add flagrant <helm repository>
helm repo update
helm install my-flagrant flagrant/flagrant --version 0.0.33 \
  --set litestream.replica.bucket=your-s3-bucket \
  --set litestream.replica.region=eu-west-1
```

## Verifying a rendered install

```sh
helm lint k8s/flagrant

helm template k8s/flagrant \
  --set litestream.replica.bucket=my-bucket \
  --set litestream.replica.region=eu-west-1
```
