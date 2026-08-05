export interface paths {
    "/databases": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** List databases */
        get: operations["Databases_list"];
        put?: never;
        /** Create a database */
        post: operations["Databases_create"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Get a database by id */
        get: operations["Databases_get"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        /** @description Request body for creating a database. */
        CreateDatabaseRequest: {
            name: string;
        };
        /** @description A provisioned MonaDB logical database. */
        Database: {
            /** @description Stable identifier used in hostnames (e.g. abc123). */
            id: string;
            /** @description Human-readable name. */
            name: string;
            /** @description SNI hostname for this database. */
            hostname: string;
            /** @description MongoDB connection URI including TLS query params for local use. */
            connectionString: string;
            status: components["schemas"]["DatabaseStatus"];
            /**
             * Format: date-time
             * @description ISO-8601 creation timestamp.
             */
            createdAt: string;
        };
        /**
         * @description Lifecycle status of a logical database.
         * @enum {string}
         */
        DatabaseStatus: "pending" | "ready" | "sleeping" | "error";
        NotFoundError: {
            detail: string;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    Databases_list: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The request has succeeded. */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Database"][];
                };
            };
        };
    };
    Databases_create: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateDatabaseRequest"];
            };
        };
        responses: {
            /** @description The request has succeeded and a new resource has been created as a result. */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Database"];
                };
            };
        };
    };
    Databases_get: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The request has succeeded. */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Database"];
                };
            };
            /** @description The server cannot find the requested resource. */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["NotFoundError"];
                };
            };
        };
    };
}
