/** Tencent Cloud TMT `X-TC-Region` values (product region list). */
export const TENCENT_REGION_OPTIONS = [
  { id: "ap-guangzhou", nameZh: "华南（广州）", nameEn: "Guangzhou" },
  { id: "ap-shanghai", nameZh: "华东（上海）", nameEn: "Shanghai" },
  { id: "ap-nanjing", nameZh: "华东（南京）", nameEn: "Nanjing" },
  { id: "ap-beijing", nameZh: "华北（北京）", nameEn: "Beijing" },
  { id: "ap-chengdu", nameZh: "西南（成都）", nameEn: "Chengdu" },
  { id: "ap-chongqing", nameZh: "西南（重庆）", nameEn: "Chongqing" },
  { id: "ap-hongkong", nameZh: "中国香港", nameEn: "Hong Kong" },
  { id: "ap-singapore", nameZh: "新加坡", nameEn: "Singapore" },
  { id: "ap-bangkok", nameZh: "曼谷", nameEn: "Bangkok" },
  { id: "ap-jakarta", nameZh: "雅加达", nameEn: "Jakarta" },
  { id: "ap-seoul", nameZh: "首尔", nameEn: "Seoul" },
  { id: "ap-tokyo", nameZh: "东京", nameEn: "Tokyo" },
  { id: "ap-shanghai-fsi", nameZh: "上海金融", nameEn: "Shanghai FSI" },
  { id: "ap-shenzhen-fsi", nameZh: "深圳金融", nameEn: "Shenzhen FSI" },
  { id: "na-ashburn", nameZh: "弗吉尼亚", nameEn: "Ashburn" },
  { id: "na-siliconvalley", nameZh: "硅谷", nameEn: "Silicon Valley" },
  { id: "eu-frankfurt", nameZh: "法兰克福", nameEn: "Frankfurt" },
  { id: "sa-saopaulo", nameZh: "圣保罗", nameEn: "São Paulo" },
] as const;

/**
 * Volcengine Translate SigV4 region. Docs fix `cn-north-1`; SDK also lists a few others.
 */
export const VOLC_REGION_OPTIONS = [
  { id: "cn-north-1", nameZh: "华北（默认）", nameEn: "China North (default)" },
  { id: "ap-singapore-1", nameZh: "新加坡", nameEn: "Singapore" },
  { id: "ap-southeast-1", nameZh: "亚太东南", nameEn: "Asia Pacific SE" },
  { id: "us-east-1", nameZh: "美东", nameEn: "US East" },
] as const;

export type RegionOption = {
  id: string;
  nameZh: string;
  nameEn: string;
};

export function regionSuggestItems(
  options: readonly RegionOption[],
  locale: string,
): Array<{ id: string; description: string }> {
  const zh = locale.toLowerCase().startsWith("zh");
  return options.map((r) => ({
    id: r.id,
    description: zh ? r.nameZh : r.nameEn,
  }));
}
