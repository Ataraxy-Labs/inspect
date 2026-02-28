export interface PrFile {
  filename: string;
  status: string;
  additions: number;
  deletions: number;
}

const NOISE_PATTERNS = [
  /\.lock$/, /package-lock\.json$/, /yarn\.lock$/, /\.min\.(js|css)$/,
  /\.map$/, /\.DS_Store$/, /\bdist\//, /\bbuild\//, /\b__generated__\//,
  /\.snap$/,
];

export function isNoiseFile(path: string): boolean {
  return NOISE_PATTERNS.some((p) => p.test(path));
}

export async function fetchPrDiff(token: string, repo: string, prNumber: number): Promise<string> {
  const resp = await fetch(`https://api.github.com/repos/${repo}/pulls/${prNumber}`, {
    headers: {
      Authorization: `token ${token}`,
      Accept: "application/vnd.github.v3.diff",
      "User-Agent": "inspect-api",
    },
  });
  if (!resp.ok) throw new Error(`GitHub diff error ${resp.status}: ${await resp.text()}`);
  return resp.text();
}

export async function fetchPrMeta(token: string, repo: string, prNumber: number) {
  const [prResp, filesResp] = await Promise.all([
    fetch(`https://api.github.com/repos/${repo}/pulls/${prNumber}`, {
      headers: { Authorization: `token ${token}`, Accept: "application/vnd.github.v3+json", "User-Agent": "inspect-api" },
    }),
    fetch(`https://api.github.com/repos/${repo}/pulls/${prNumber}/files?per_page=100`, {
      headers: { Authorization: `token ${token}`, Accept: "application/vnd.github.v3+json", "User-Agent": "inspect-api" },
    }),
  ]);

  if (!prResp.ok) throw new Error(`GitHub PR error ${prResp.status}: ${await prResp.text()}`);
  const pr = await prResp.json();
  const files: any[] = filesResp.ok ? await filesResp.json() : [];

  return {
    number: pr.number as number,
    title: pr.title as string,
    state: pr.state as string,
    additions: pr.additions as number,
    deletions: pr.deletions as number,
    changed_files: pr.changed_files as number,
    files: files.map((f: any) => ({
      filename: f.filename,
      status: f.status,
      additions: f.additions,
      deletions: f.deletions,
    })) as PrFile[],
  };
}
