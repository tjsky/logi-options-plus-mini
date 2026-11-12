#!/usr/bin/env node

// Star History 自托管生成脚本（平滑曲线版）
// 改造自 knife4j-next 的 .github/scripts/star-history.mjs（MIT，作者 songxychn）
// 变更点：折线（阶梯 H/V）改为 Fritsch-Carlson 单调三次样条曲线，与 d3 curveMonotoneX 同族，
// 保证曲线不产生上冲/下冲（不会画出超过数据范围的"假峰"）。

import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const DAY = 24 * 60 * 60 * 1000;

function mergePoint(points, point) {
  return [...new Map([...points, point].map((item) => [item.date, item])).values()].sort(
    (left, right) => left.date.localeCompare(right.date),
  );
}

function pointsFromStargazers(stargazers) {
  const perDay = new Map();
  for (const stargazer of stargazers) {
    const date = stargazer.starred_at?.slice(0, 10);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date ?? "")) {
      throw new Error("GitHub returned a stargazer without starred_at");
    }
    perDay.set(date, (perDay.get(date) ?? 0) + 1);
  }

  let stars = 0;
  return [...perDay.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([date, count]) => ({ date, stars: (stars += count) }));
}

function niceMaximum(value) {
  if (value <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  return [1, 2, 5, 10].map((step) => step * magnitude).find((candidate) => candidate >= value);
}

function escapeXml(value) {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&apos;",
  })[character]);
}

/**
 * Fritsch-Carlson 单调三次插值，输出 SVG 三次贝塞尔路径。
 * coordinates 必须按 x 升序排列且 x 严格递增。
 * 与 d3 的 curveMonotoneX 同族：曲线经过所有数据点，且在单调区间内不会越过数据值。
 */
function monotonePath(coordinates) {
  const n = coordinates.length;
  if (n === 0) return "";
  if (n === 1) {
    return `M ${coordinates[0].x.toFixed(1)} ${coordinates[0].y.toFixed(1)}`;
  }

  const dx = [];
  const slope = [];
  for (let i = 0; i < n - 1; i += 1) {
    dx[i] = coordinates[i + 1].x - coordinates[i].x;
    slope[i] = dx[i] === 0
      ? 0
      : (coordinates[i + 1].y - coordinates[i].y) / dx[i];
  }

  const tangent = new Array(n);
  tangent[0] = slope[0];
  tangent[n - 1] = slope[n - 2];
  for (let i = 1; i < n - 1; i += 1) {
    if (slope[i - 1] * slope[i] <= 0) {
      tangent[i] = 0; // 极值点处切线水平，避免过冲
    } else {
      tangent[i] = (slope[i - 1] + slope[i]) / 2;
    }
  }

  // Fritsch-Carlson 限幅，进一步抑制过冲
  for (let i = 0; i < n - 1; i += 1) {
    if (slope[i] === 0) {
      tangent[i] = 0;
      tangent[i + 1] = 0;
      continue;
    }
    const a = tangent[i] / slope[i];
    const b = tangent[i + 1] / slope[i];
    const s = a * a + b * b;
    if (s > 9) {
      const t = 3 / Math.sqrt(s);
      tangent[i] = t * a * slope[i];
      tangent[i + 1] = t * b * slope[i];
    }
  }

  let d = `M ${coordinates[0].x.toFixed(1)} ${coordinates[0].y.toFixed(1)}`;
  for (let i = 0; i < n - 1; i += 1) {
    const h = dx[i] / 3;
    d += ` C ${(coordinates[i].x + h).toFixed(1)} ${(coordinates[i].y + tangent[i] * h).toFixed(1)},`
      + ` ${(coordinates[i + 1].x - h).toFixed(1)} ${(coordinates[i + 1].y - tangent[i + 1] * h).toFixed(1)},`
      + ` ${coordinates[i + 1].x.toFixed(1)} ${coordinates[i + 1].y.toFixed(1)}`;
  }
  return d;
}

function renderSvg(history) {
  const width = 960;
  const height = 520;
  const left = 76;
  const right = 32;
  const top = 92;
  const bottom = 64;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const times = history.points.map(({ date }) => Date.parse(`${date}T00:00:00Z`));
  const minimumTime = Math.min(...times);
  const maximumTime = Math.max(...times);
  const timeSpan = Math.max(maximumTime - minimumTime, DAY);
  const maximumStars = niceMaximum(Math.max(...history.points.map(({ stars }) => stars)));
  const x = (time) => history.points.length === 1
    ? width - right
    : left + ((time - minimumTime) / timeSpan) * plotWidth;
  const y = (stars) => top + plotHeight - (stars / maximumStars) * plotHeight;
  const coordinates = history.points.map((point, index) => ({
    x: x(times[index]),
    y: y(point.stars),
  }));
  const line = monotonePath(coordinates);
  const area = coordinates.length > 1
    ? `${line} V ${height - bottom} H ${coordinates[0].x.toFixed(1)} Z`
    : "";
  const number = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });
  const date = new Intl.DateTimeFormat("en", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  });
  const grid = Array.from({ length: 6 }, (_, index) => {
    const stars = (maximumStars * index) / 5;
    const position = y(stars);
    return `<path class="grid" d="M ${left} ${position.toFixed(1)} H ${width - right}"/><text class="label" x="${left - 12}" y="${(position + 4).toFixed(1)}" text-anchor="end">${number.format(stars)}</text>`;
  }).join("");
  const dates = Array.from({ length: 5 }, (_, index) => {
    const time = minimumTime + (timeSpan * index) / 4;
    const position = left + (plotWidth * index) / 4;
    const anchor = index === 0 ? "start" : index === 4 ? "end" : "middle";
    return `<path class="grid" d="M ${position.toFixed(1)} ${top} V ${height - bottom}"/><text class="label" x="${position.toFixed(1)}" y="${height - bottom + 28}" text-anchor="${anchor}">${date.format(time)}</text>`;
  }).join("");
  const latest = history.points.at(-1);

  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title description">
  <title id="title">${escapeXml(history.repository)} Star History</title>
  <desc id="description">GitHub stars by date, last updated ${history.updated}</desc>
  <style>
    .background { fill: #ffffff; }
    .grid { fill: none; stroke: #d8dee4; stroke-width: 1; }
    .label, .subtitle { fill: #57606a; font: 13px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    .heading, .value { fill: #24292f; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    .heading { font-size: 20px; font-weight: 600; }
    .value { font-size: 24px; font-weight: 600; }
    .area { fill: #0969da; opacity: .12; }
    .line { fill: none; stroke: #0969da; stroke-linecap: round; stroke-linejoin: round; stroke-width: 3; }
    .point { fill: #0969da; stroke: #ffffff; stroke-width: 2; }
    @media (prefers-color-scheme: dark) {
      .background { fill: #0d1117; }
      .grid { stroke: #30363d; }
      .label, .subtitle { fill: #8b949e; }
      .heading, .value { fill: #f0f6fc; }
      .area { fill: #58a6ff; }
      .line { stroke: #58a6ff; }
      .point { fill: #58a6ff; stroke: #0d1117; }
    }
  </style>
  <rect class="background" width="${width}" height="${height}" rx="8"/>
  <text class="heading" x="${left}" y="38">${escapeXml(history.repository)} Star History</text>
  <text class="subtitle" x="${left}" y="62">GitHub stars by date · updated ${history.updated}</text>
  <text class="value" x="${width - right}" y="40" text-anchor="end">★ ${number.format(latest.stars)}</text>
  ${grid}
  ${dates}
  <path class="area" d="${area}"/>
  <path class="line" d="${line}"/>
  <circle class="point" cx="${coordinates.at(-1).x.toFixed(1)}" cy="${coordinates.at(-1).y.toFixed(1)}" r="5"/>
</svg>
`;
}

async function github(pathname, token, accept = "application/vnd.github+json", apiVersion = "2022-11-28") {
  const headers = {
    Accept: accept,
    "User-Agent": "star-history-action",
    "X-GitHub-Api-Version": apiVersion,
  };
  if (token) headers.Authorization = `Bearer ${token}`;
  const response = await fetch(`https://api.github.com${pathname}`, { headers });
  if (!response.ok) {
    throw new Error(`GitHub API ${response.status}: ${(await response.text()).slice(0, 300)}`);
  }
  return response.json();
}

/**
 * 聚合历史端点（2026-07 新增，公开仓库无需认证）：
 * GET /repos/{owner}/{repo}/stargazers/history
 * 返回按周分桶的星标增量（不暴露 stargazer 身份），最新一周在前。
 */
async function fetchStarHistoryBuckets(repository, token) {
  const buckets = [];
  for (let page = 1; ; page += 1) {
    const batch = await github(
      `/repos/${repository}/stargazers/history?per_page=30&page=${page}`,
      token,
      "application/vnd.github+json",
      "2026-03-10",
    );
    buckets.push(...batch);
    if (batch.length < 30) return buckets;
  }
}

function pointsFromHistoryBuckets(buckets, today) {
  const daily = [];
  for (const bucket of [...buckets].sort((left, right) => left.week - right.week)) {
    for (let index = 0; index < 7; index += 1) {
      const date = new Date(bucket.week * 1000 + index * DAY).toISOString().slice(0, 10);
      if (date > today) continue; // 当前周的未到日期
      daily.push({ date, added: bucket.days[index] ?? 0 });
    }
  }
  let stars = 0;
  return daily.map(({ date, added }) => ({ date, stars: (stars += added) }));
}

async function fetchStargazers(repository, token) {
  const stargazers = [];
  for (let page = 1; ; page += 1) {
    const batch = await github(
      `/repos/${repository}/stargazers?per_page=100&page=${page}`,
      token,
      "application/vnd.github.star+json",
    );
    stargazers.push(...batch);
    if (batch.length < 100) return stargazers;
  }
}

async function readHistory(file, repository) {
  try {
    const history = JSON.parse(await readFile(file, "utf8"));
    if (history.repository !== repository || !Array.isArray(history.points)) {
      throw new Error(`${file} does not contain history for ${repository}`);
    }
    for (const point of history.points) {
      if (!/^\d{4}-\d{2}-\d{2}$/.test(point.date) || !Number.isInteger(point.stars) || point.stars < 0) {
        throw new Error(`${file} contains an invalid data point`);
      }
    }
    return history;
  } catch (error) {
    if (error.code === "ENOENT") return { repository, points: [] };
    throw error;
  }
}

function selfTest() {
  assert.deepEqual(
    mergePoint([{ date: "2026-07-17", stars: 10 }], { date: "2026-07-17", stars: 11 }),
    [{ date: "2026-07-17", stars: 11 }],
  );
  assert.deepEqual(pointsFromStargazers([
    { starred_at: "2026-07-17T01:00:00Z" },
    { starred_at: "2026-07-18T01:00:00Z" },
    { starred_at: "2026-07-18T02:00:00Z" },
  ]), [
    { date: "2026-07-17", stars: 1 },
    { date: "2026-07-18", stars: 3 },
  ]);

  // 周桶聚合（最新在前）转日累计点；零增量日保留，当前周未到的日期跳过
  assert.deepEqual(pointsFromHistoryBuckets([
    { week: Date.parse("2026-07-12T00:00:00Z") / 1000, total: 3, days: [0, 1, 0, 2, 0, 0, 0] },
    { week: Date.parse("2026-07-19T00:00:00Z") / 1000, total: 2, days: [1, 1, 0, 0, 0, 0, 0] },
  ], "2026-07-20"), [
    { date: "2026-07-12", stars: 0 },
    { date: "2026-07-13", stars: 1 },
    { date: "2026-07-14", stars: 1 },
    { date: "2026-07-15", stars: 3 },
    { date: "2026-07-16", stars: 3 },
    { date: "2026-07-17", stars: 3 },
    { date: "2026-07-18", stars: 3 },
    { date: "2026-07-19", stars: 4 },
    { date: "2026-07-20", stars: 5 },
  ]);

  // 单调递增数据：曲线 y 值不得越过相邻数据点（单调性检查）
  const svg = renderSvg({
    repository: "owner/repo",
    updated: "2026-07-18",
    points: [
      { date: "2026-05-01", stars: 3 },
      { date: "2026-05-20", stars: 12 },
      { date: "2026-06-10", stars: 15 },
      { date: "2026-07-01", stars: 40 },
      { date: "2026-07-18", stars: 43 },
    ],
  });
  assert.match(svg, /owner\/repo Star History/);
  assert.match(svg, /★ 43/);
  assert.match(svg, / C /, "path should contain cubic Bezier segments (smooth curve)");

  // 单点数据：只画一个点，不报错
  const single = renderSvg({
    repository: "owner/repo",
    updated: "2026-07-18",
    points: [{ date: "2026-07-18", stars: 3 }],
  });
  assert.match(single, /★ 3/);

  // 带回落数据（先增后减）：限幅后不应产生NaN或过冲路径异常
  const dip = renderSvg({
    repository: "owner/repo",
    updated: "2026-07-18",
    points: [
      { date: "2026-06-01", stars: 10 },
      { date: "2026-06-15", stars: 20 },
      { date: "2026-07-01", stars: 5 },
      { date: "2026-07-18", stars: 8 },
    ],
  });
  assert.doesNotMatch(dip, /NaN/);
  console.log("star history self-test passed");
}

async function main() {
  const [command = "snapshot", outputDirectory = "."] = process.argv.slice(2);
  if (command === "self-test") return selfTest();
  if (!new Set(["snapshot", "bootstrap"]).has(command)) {
    throw new Error("usage: star-history.mjs [snapshot|bootstrap|self-test] [output-directory]");
  }

  const repository = process.env.GITHUB_REPOSITORY;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "")) {
    throw new Error("GITHUB_REPOSITORY must be in owner/repository form");
  }
  const token = process.env.STAR_HISTORY_TOKEN ?? process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN;

  const file = path.join(outputDirectory, "history.json");
  const history = await readHistory(file, repository);
  const repositoryData = await github(`/repos/${repository}`, token);
  const today = new Date().toISOString().slice(0, 10);
  let points;
  if (command === "bootstrap") {
    try {
      // 优先用聚合历史端点（公开仓库无需 Token）
      const buckets = await fetchStarHistoryBuckets(repository, token);
      points = pointsFromHistoryBuckets(buckets, today);
      if (points.length === 0) throw new Error("aggregate history endpoint returned no data");
      // 曲线尾值对齐当前实际 Star 数（补偿取消星标造成的偏差）
      const delta = repositoryData.stargazers_count - points.at(-1).stars;
      if (delta !== 0) {
        points = points.map((point) => ({ date: point.date, stars: point.stars + delta }));
        console.log(`aligned history tail to current count (offset ${delta >= 0 ? "+" : ""}${delta})`);
      }
    } catch (error) {
      // 回退：Stargazer 列表（2026-07 起需要仓库管理员/协作者 Token）
      if (!token) {
        throw new Error(
          `aggregate history endpoint failed (${error.message}); ` +
          "falling back to stargazers listing requires STAR_HISTORY_TOKEN, GITHUB_TOKEN, or GH_TOKEN",
        );
      }
      console.log(`aggregate history endpoint failed (${error.message}); using stargazers listing fallback`);
      points = pointsFromStargazers(await fetchStargazers(repository, token));
    }
  } else {
    points = history.points;
  }
  points = mergePoint(points, { date: today, stars: repositoryData.stargazers_count });
  const updated = { repository, updated: today, points };

  await mkdir(outputDirectory, { recursive: true });
  await Promise.all([
    writeFile(file, `${JSON.stringify(updated, null, 2)}\n`),
    writeFile(path.join(outputDirectory, "star-history.svg"), renderSvg(updated)),
  ]);
  console.log(`${command}: wrote ${points.length} points for ${repository}`);
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
