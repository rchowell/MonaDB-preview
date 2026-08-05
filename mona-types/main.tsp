import "@typespec/http";
import "@typespec/rest";
import "@typespec/openapi";
import "@typespec/openapi3";

using TypeSpec.Http;
using TypeSpec.Rest;
using TypeSpec.OpenAPI;

@service(#{ title: "MonaDB Control Plane" })
@server("http://localhost:8000", "Local development")
namespace MonaDB;

@doc("Lifecycle status of a logical database.")
enum DatabaseStatus {
  pending,
  ready,
  sleeping,
  error,
}

@doc("A provisioned MonaDB logical database.")
model Database {
  @doc("Stable identifier used in hostnames (e.g. abc123).")
  id: string;

  @doc("Human-readable name.")
  name: string;

  @doc("SNI hostname for this database.")
  hostname: string;

  @doc("MongoDB connection URI including TLS query params for local use.")
  connectionString: string;

  status: DatabaseStatus;

  @doc("ISO-8601 creation timestamp.")
  createdAt: utcDateTime;
}

@doc("Request body for creating a database.")
model CreateDatabaseRequest {
  @minLength(1)
  @maxLength(64)
  name: string;
}

@route("/databases")
@tag("Databases")
interface Databases {
  @get
  @summary("List databases")
  list(): Database[];

  @post
  @summary("Create a database")
  create(@body body: CreateDatabaseRequest): {
    @statusCode statusCode: 201;
    @body body: Database;
  };

  @get
  @route("/{id}")
  @summary("Get a database by id")
  get(@path id: string): Database | {
    @statusCode statusCode: 404;
    @body body: NotFoundError;
  };
}

model NotFoundError {
  detail: string;
}
