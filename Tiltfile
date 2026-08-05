# MonaDB local full stack (kind + control plane + gateway + edge + Next.js console)
# Usage: tilt up

CLUSTER_NAME = 'mona'
CONTEXT = 'kind-mona'
K8S = 'infra/k8s'

allow_k8s_contexts(CONTEXT)


def need(cmd):
    local(
        'command -v %s >/dev/null 2>&1 || { echo "missing required command: %s" >&2; exit 1; }' % (cmd, cmd),
        quiet=True,
    )


def ensure_kind_cluster():
    need('kind')
    need('kubectl')
    need('docker')
    need('openssl')

    clusters = str(local(['kind', 'get', 'clusters'], quiet=True)).strip().splitlines()
    if CLUSTER_NAME not in clusters:
        print('Creating kind cluster %s...' % CLUSTER_NAME)
        local(['kind', 'create', 'cluster', '--config', '%s/kind.yaml' % K8S])

    # kind create writes kubeconfig, but an existing cluster may have been created
    # without this machine's kubeconfig (or it was overwritten). Always re-export
    # so Tilt talks to kind-mona — not docker-desktop — where host ports map.
    local(['kind', 'export', 'kubeconfig', '--name', CLUSTER_NAME], quiet=True)
    local(['kubectl', 'config', 'use-context', CONTEXT], quiet=True)
    local(['kubectl', 'cluster-info', '--context', CONTEXT], quiet=True)


ensure_kind_cluster()

# TLS for the edge SNI proxy (*.mona.localhost)
local(['bash', '%s/scripts/gen-certs.sh' % K8S])
watch_file('%s/certs/tls.crt' % K8S)
watch_file('%s/certs/tls.key' % K8S)
k8s_yaml(
    local([
        'kubectl', 'create', 'secret', 'tls', 'mona-edge-tls',
        '--namespace=mona',
        '--cert=%s/certs/tls.crt' % K8S,
        '--key=%s/certs/tls.key' % K8S,
        '--dry-run=client', '-o', 'yaml',
    ])
)

k8s_yaml([
    '%s/base/namespace.yaml' % K8S,
    '%s/base/rbac.yaml' % K8S,
    '%s/base/postgres.yaml' % K8S,
    '%s/base/mona-api.yaml' % K8S,
    '%s/base/mona-gateway.yaml' % K8S,
    '%s/base/mona-edge.yaml' % K8S,
])

docker_build(
    'mona-api:local',
    'mona-api',
    ignore=['target', '.git', 'templates'],
)

# Gateway links monadb as a path dependency; build from repo root.
docker_build(
    'mona-gateway:local',
    '.',
    dockerfile='mona-gateway/Dockerfile',
    only=['mona-db', 'mona-gateway'],
    ignore=['**/target', 'mona-db/docs', 'mona-db/scripts', '**/.git'],
)

docker_build(
    'mona-edge:local',
    'mona-edge',
    ignore=['target', '.git'],
)

# Host ports come from kind.yaml NodePort mappings (8000, 27017).
k8s_resource(
    'postgres',
    labels=['data'],
)

k8s_resource(
    'mona-api',
    resource_deps=['postgres'],
    labels=['control-plane'],
    links=['http://localhost:8000/docs'],
)

k8s_resource(
    'mona-gateway',
    resource_deps=['mona-api'],
    labels=['gateway'],
)

k8s_resource(
    'mona-edge',
    resource_deps=['mona-gateway'],
    labels=['edge'],
)

local_resource(
    'mona-app',
    cmd='pnpm install',
    dir='mona-app',
    serve_cmd='pnpm run dev',
    serve_dir='mona-app',
    serve_env={
        'NEXT_PUBLIC_MONA_API_URL': 'http://localhost:8000',
    },
    deps=['mona-app/package.json', 'mona-app/package-lock.json'],
    resource_deps=['mona-api'],
    labels=['frontend'],
    links=['http://localhost:3000'],
    readiness_probe=probe(
        http_get=http_get_action(port=3000, path='/'),
        period_secs=5,
        timeout_secs=3,
        failure_threshold=30,
    ),
)
