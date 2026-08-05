# MonaDB local full stack (kind + control plane + edge + Next.js console)
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

    local(['kubectl', 'config', 'use-context', CONTEXT], quiet=True)
    local(['kubectl', 'cluster-info', '--context', CONTEXT], quiet=True)


def sync_api_templates():
    # mona-api image expects templates/ in its build context (same as up.sh).
    local('rm -rf mona-api/templates && cp -R %s/templates mona-api/templates' % K8S)
    for name in ['deployment.yaml', 'pvc.yaml', 'service.yaml']:
        watch_file('%s/templates/%s' % (K8S, name))


ensure_kind_cluster()
sync_api_templates()

# TLS for the edge SNI proxy (*.mona.local)
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
    '%s/base/edge.yaml' % K8S,
])

# Engine image is spawned by mona-api (MONADB_IMAGE), not a static Deployment.
docker_build(
    'mona-db:local',
    'mona-db',
    match_in_env_vars=True,
    ignore=['docs', 'scripts', 'target', '.git'],
)

docker_build(
    'mona-api:local',
    'mona-api',
    ignore=['.venv', '**/__pycache__', '**/*.pyc', '.git'],
)

docker_build(
    'mona-edge:local',
    '%s/edge' % K8S,
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
    'mona-edge',
    resource_deps=['mona-api'],
    labels=['edge'],
)

local_resource(
    'mona-app',
    cmd='npm install',
    dir='mona-app',
    serve_cmd='npm run dev',
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
