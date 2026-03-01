export interface PrInfo {
  number: number;
  title: string;
  state: string;
  additions: number;
  deletions: number;
  changed_files: number;
  files: PrFile[];
}

export interface PrFile {
  filename: string;
  status: string;
  additions: number;
  deletions: number;
}

const NOISE_PATTERNS = [
  /\.lock$/,
  /package-lock\.json$/,
  /yarn\.lock$/,
  /\.min\.(js|css)$/,
  /\.map$/,
  /\.DS_Store$/,
  /\bdist\//,
  /\bbuild\//,
  /\b__generated__\//,
  /\.snap$/,
];

export function isNoiseFile(path: string): boolean {
  return NOISE_PATTERNS.some((p) => p.test(path));
}

export async function fetchPr(
  token: string,
  repo: string,
  prNumber: number
): Promise<PrInfo> {
  const resp = await fetch(
    `https://api.github.com/repos/${repo}/pulls/${prNumber}`,
    {
      headers: {
        Authorization: `token ${token}`,
        Accept: "application/vnd.github.v3+json",
        "User-Agent": "inspect-api",
      },
    }
  );

  if (!resp.ok) {
    const text = await resp.text();
    throw new Error(`GitHub API error ${resp.status}: ${text}`);
  }

  const data = await resp.json();

  // Fetch files separately (PR endpoint doesn't always include all files)
  const filesResp = await fetch(
    `https://api.github.com/repos/${repo}/pulls/${prNumber}/files?per_page=100`,
    {
      headers: {
        Authorization: `token ${token}`,
        Accept: "application/vnd.github.v3+json",
        "User-Agent": "inspect-api",
      },
    }
  );

  const files = filesResp.ok ? await filesResp.json() : [];

  return {
    number: data.number,
    title: data.title,
    state: data.state,
    additions: data.additions,
    deletions: data.deletions,
    changed_files: data.changed_files,
    files: files.map((f: any) => ({
      filename: f.filename,
      status: f.status,
      additions: f.additions,
      deletions: f.deletions,
    })),
  };
}

export async function fetchPrDiff(
  token: string,
  repo: string,
  prNumber: number
): Promise<string> {
  const resp = await fetch(
    `https://api.github.com/repos/${repo}/pulls/${prNumber}`,
    {
      headers: {
        Authorization: `token ${token}`,
        Accept: "application/vnd.github.v3.diff",
        "User-Agent": "inspect-api",
      },
    }
  );

  if (!resp.ok) {
    const text = await resp.text();
    throw new Error(`GitHub diff error ${resp.status}: ${text}`);
  }

  return resp.text();
}
