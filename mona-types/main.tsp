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

@doc("Request body for updating a database.")
model UpdateDatabaseRequest {
  @minLength(1)
  @maxLength(64)
  name: string;
}

@doc("Error payload returned for 4xx/5xx responses (`{ detail: string }`).")
model ErrorBody {
  detail: string;
}

@route("/databases")
@tag("Databases")
interface Databases {
  @get
  @summary("List databases")
  list(): Database[];

  @post
  @summary("Create a database")
  @doc("Returns 400 with ErrorBody when the name is invalid.")
  create(@body body: CreateDatabaseRequest): {
    @statusCode statusCode: 201;
    @body body: Database;
  };

  @get
  @route("/{id}")
  @summary("Get a database by id")
  @doc("Returns 404 with ErrorBody when the database does not exist.")
  get(@path id: string): Database;

  @patch
  @route("/{id}")
  @summary("Update a database")
  @doc("Returns 400 with ErrorBody for invalid names, or 404 when missing.")
  update(@path id: string, @body body: UpdateDatabaseRequest): Database;

  @delete
  @route("/{id}")
  @summary("Delete a database")
  @doc("Returns 404 with ErrorBody when the database does not exist.")
  delete(@path id: string): {
    @statusCode statusCode: 204;
  };
}
