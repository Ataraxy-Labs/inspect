/**
 * Web tools extension — web_search + read_web_page via Parallel AI.
 *
 * Provides both tools from Amp's code-review config.
 * Requires PARALLEL_API_KEY environment variable.
 */

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import Parallel from "parallel-web";

export default function (pi: ExtensionAPI) {
  const apiKey = process.env.PARALLEL_API_KEY;

  if (!apiKey) {
    pi.on("session_start", async (_event, ctx) => {
      if (ctx.hasUI) {
        ctx.ui.notify("web tools: PARALLEL_API_KEY not set — tools disabled", "warning");
      }
    });
    return;
  }

  const client = new Parallel({ apiKey });

  // ── web_search ──
  pi.registerTool({
    name: "web_search",
    label: "Web Search",
    description:
      "Search the web for information relevant to a research objective.\n\n" +
      "Use when you need up-to-date or precise documentation. Use `read_web_page` to fetch full content from a specific URL.\n\n" +
      "# Examples\n\n" +
      "Get API documentation for a specific provider\n" +
      '```json\n{"objective":"I want to know the request fields for the Stripe billing create customer API. Prefer Stripe\'s docs site."}\n```\n\n' +
      "See usage documentation for newly released library features\n" +
      '```json\n{"objective":"I want to know how to use SvelteKit remote functions, which is a new feature shipped in the last month.","search_queries":["sveltekit","remote function"]}\n```',
    parameters: Type.Object({
      objective: Type.String({
        description: "A natural-language description of the broader task or research goal, including any source or freshness guidance",
      }),
      search_queries: Type.Array(Type.String(), {
        description: "Optional keyword queries to ensure matches for specific terms are prioritized (recommended for best results)",
        minItems: 1,
        maxItems: 5,
      }),
      max_results: Type.Optional(
        Type.Number({
          description: "The maximum number of results to return (default: 5)",
          minimum: 1,
          maximum: 20,
        }),
      ),
    }),

    async execute(_toolCallId, params, signal, onUpdate) {
      const { objective, search_queries, max_results = 5 } = params;

      onUpdate?.({
        content: [{ type: "text", text: `Searching: ${search_queries.join(", ")}` }],
        details: {},
      });

      let search;
      try {
        search = await client.beta.search({
          objective,
          search_queries,
          max_results,
          max_chars_per_result: 10000,
        });
      } catch (err: any) {
        return {
          content: [{ type: "text", text: `Search failed: ${err.message}` }],
          details: {},
        };
      }

      if (signal?.aborted) {
        return {
          content: [{ type: "text", text: "Search cancelled" }],
          details: {},
        };
      }

      const results = search.results ?? [];

      if (results.length === 0) {
        return {
          content: [{ type: "text", text: "No results found." }],
          details: {},
        };
      }

      const resultText = results
        .map((r: any, i: number) => {
          const parts = [`## Result ${i + 1}`];
          if (r.title) parts.push(`**Title:** ${r.title}`);
          if (r.url) parts.push(`**URL:** ${r.url}`);
          if (r.content) parts.push(`\n${r.content}`);
          else if (r.text) parts.push(`\n${r.text}`);
          else if (r.snippet) parts.push(`\n${r.snippet}`);
          return parts.join("\n");
        })
        .join("\n\n---\n\n");

      return {
        content: [{ type: "text", text: resultText }],
        details: {},
      };
    },
  });

  // ── read_web_page ──
  pi.registerTool({
    name: "read_web_page",
    label: "Read Web Page",
    description:
      "Read the contents of a web page at a given URL.\n\n" +
      "When only the url parameter is set, it returns the contents of the webpage converted to Markdown.\n\n" +
      "When an objective is provided, it returns excerpts relevant to that objective.\n\n" +
      "If the user asks for the latest or recent contents, pass `forceRefetch: true` to ensure the latest content is fetched.\n\n" +
      "Do NOT use for access to localhost or any other local or non-Internet-accessible URLs; use `curl` via the Bash instead.\n\n" +
      "# Examples\n\n" +
      "Summarize recent changes for a library. Force refresh because freshness is important.\n" +
      '```json\n{"url":"https://example.com/changelog","objective":"Summarize the API changes in this software library.","forceRefetch":true}\n```\n\n' +
      "Extract all text content from a web page\n" +
      '```json\n{"url":"https://example.com/docs/getting-started"}\n```',
    parameters: Type.Object({
      url: Type.String({
        description: "The URL of the web page to read",
      }),
      objective: Type.Optional(
        Type.String({
          description:
            "A natural-language description of the research goal. If set, only relevant excerpts will be returned. If not set, the full content of the web page will be returned.",
        }),
      ),
      forceRefetch: Type.Optional(
        Type.Boolean({
          description: "Force a live fetch of the URL (default: use a cached version that may be a few days old)",
        }),
      ),
    }),

    async execute(_toolCallId, params, signal, onUpdate) {
      const { url, objective } = params;

      onUpdate?.({
        content: [{ type: "text", text: `Fetching: ${url}` }],
        details: {},
      });

      try {
        // Use Parallel's search with the URL as a query to get page content
        const result = await client.beta.search({
          objective: objective ?? `Read and return the full contents of this page: ${url}`,
          search_queries: [url],
          max_results: 1,
          max_chars_per_result: 50000,
        });

        if (signal?.aborted) {
          return {
            content: [{ type: "text", text: "Fetch cancelled" }],
            details: {},
          };
        }

        const page = result.results?.[0];
        if (!page) {
          return {
            content: [{ type: "text", text: `No content returned for ${url}` }],
            details: {},
          };
        }

        const parts: string[] = [];
        if (page.title) parts.push(`# ${page.title}`);
        if (page.url) parts.push(`**URL:** ${page.url}`);
        const body = page.content ?? page.text ?? page.snippet ?? "";
        if (body) parts.push(`\n${body}`);

        return {
          content: [{ type: "text", text: parts.join("\n") || `No content found at ${url}` }],
          details: {},
        };
      } catch (err: any) {
        return {
          content: [{ type: "text", text: `Failed to fetch ${url}: ${err.message}` }],
          details: {},
        };
      }
    },
  });
}
