import { describe, expect, it } from "vitest";
import { checkExpiry, formatRelativeExpiry, getItemExpiry, isExpiryField, parseDate, parseExpiry, parseMonthYear } from "./expiry";
import { translate } from "../../i18n";

// Fixed "now": May 15, 2026, 12:00:00 UTC
const NOW = new Date("2026-05-15T12:00:00.000Z");

describe("expiry helper", () => {
  describe("parseMonthYear", () => {
    it("parses MM/YYYY and counts valid until the end of that month", () => {
      const d = parseMonthYear("05/2026");
      expect(d?.toISOString()).toBe("2026-05-31T23:59:59.999Z");
    });

    it("parses YYYY/MM and YYYY-MM", () => {
      expect(parseMonthYear("2026/05")?.toISOString()).toBe("2026-05-31T23:59:59.999Z");
      expect(parseMonthYear("2026-05")?.toISOString()).toBe("2026-05-31T23:59:59.999Z");
    });

    it("parses 2-digit years MM/YY", () => {
      expect(parseMonthYear("05/26")?.toISOString()).toBe("2026-05-31T23:59:59.999Z");
    });

    it("handles leap years correctly for February", () => {
      expect(parseMonthYear("02/2028")?.toISOString()).toBe("2028-02-29T23:59:59.999Z");
      expect(parseMonthYear("02/2026")?.toISOString()).toBe("2026-02-28T23:59:59.999Z");
    });

    it("returns null for garbage input", () => {
      expect(parseMonthYear("")).toBeNull();
      expect(parseMonthYear("not-a-date")).toBeNull();
      expect(parseMonthYear("13/2026")).toBeNull();
      expect(parseMonthYear("00/2026")).toBeNull();
      expect(parseMonthYear("05/1800")).toBeNull();
    });
  });

  describe("parseDate", () => {
    it("parses YYYY-MM-DD and counts valid until end of day", () => {
      const d = parseDate("2026-10-31");
      expect(d?.toISOString()).toBe("2026-10-31T23:59:59.999Z");
    });

    it("parses ISO strings with time", () => {
      const d = parseDate("2026-06-01T10:00:00.000Z");
      expect(d?.toISOString()).toBe("2026-06-01T10:00:00.000Z");
    });

    it("returns null for garbage or impossible dates", () => {
      expect(parseDate("")).toBeNull();
      expect(parseDate("garbage")).toBeNull();
      expect(parseDate("2026-02-31")).toBeNull();
      expect(parseDate("2026-13-01")).toBeNull();
    });
  });

  describe("acceptance criteria: checkExpiry with fixed now", () => {
    it("past month (month_year) -> expired", () => {
      // April 2026 expired on 2026-04-30, now is 2026-05-15
      const res = checkExpiry("04/2026", "month_year", NOW);
      expect(res).not.toBeNull();
      expect(res?.status).toBe("expired");
      expect(res?.relativeHint).toBe("Expired 15 days ago");
    });

    it("past month several months ago -> expired with months relative hint", () => {
      // March 2026 expired end of March (2 months before May)
      const res = checkExpiry("03/2026", "month_year", NOW);
      expect(res).not.toBeNull();
      expect(res?.status).toBe("expired");
      expect(res?.relativeHint).toBe("Expired 2 months ago");
    });

    it("current month (month_year) -> expiring_soon", () => {
      // May 2026 expires on 2026-05-31, which is +16 days from May 15
      const res = checkExpiry("05/2026", "month_year", NOW);
      expect(res).not.toBeNull();
      expect(res?.status).toBe("expiring_soon");
      expect(res?.relativeHint).toBe("Expires in 16 days");
    });

    it("+29 days (date) -> expiring_soon", () => {
      // NOW is 2026-05-15T12:00:00Z. +29 days end of day is June 13
      const res = checkExpiry("2026-06-13", "date", NOW);
      expect(res).not.toBeNull();
      expect(res?.status).toBe("expiring_soon");
      expect(res?.relativeHint).toBe("Expires in 29 days");
    });

    it("+31 days (date) -> null (neither expired nor expiring soon)", () => {
      // June 16 is 32 days away
      const res = checkExpiry("2026-06-16", "date", NOW);
      expect(res).toBeNull();
    });

    it("+12 days (date) -> expiring_soon with relative hint", () => {
      // May 27 is 12 days away
      const res = checkExpiry("2026-05-27", "date", NOW);
      expect(res).not.toBeNull();
      expect(res?.status).toBe("expiring_soon");
      expect(res?.relativeHint).toBe("Expires in 12 days");
    });

    it("date in the past -> expired", () => {
      const res = checkExpiry("2026-05-01", "date", NOW);
      expect(res).not.toBeNull();
      expect(res?.status).toBe("expired");
      expect(res?.relativeHint).toBe("Expired 14 days ago");
    });

    it("garbage input -> null", () => {
      expect(checkExpiry("garbage", "month_year", NOW)).toBeNull();
      expect(checkExpiry("not-a-date", "date", NOW)).toBeNull();
      expect(checkExpiry("", undefined, NOW)).toBeNull();
      expect(checkExpiry(null, undefined, NOW)).toBeNull();
      expect(checkExpiry(undefined, undefined, NOW)).toBeNull();
    });
  });

  describe("getItemExpiry and isExpiryField", () => {
    it("recognizes credit card expiry fields", () => {
      expect(isExpiryField({ id: "expiry", label: "expiry date", kind: "month_year" }, "credit_card")).toBe(true);
      expect(isExpiryField({ id: "valid_from", label: "valid from", kind: "month_year" }, "credit_card")).toBe(false);
    });

    it("recognizes API credential expires fields", () => {
      expect(isExpiryField({ id: "expires", label: "expires", kind: "date" }, "api_credential")).toBe(true);
      expect(isExpiryField({ id: "birth", label: "birth date", kind: "date" }, "identity")).toBe(false);
    });

    it("gets expiry from item with summary expiry field", () => {
      const item = {
        category: "credit_card" as const,
        expiry: "03/2026",
      };
      const res = getItemExpiry(item, NOW);
      expect(res?.status).toBe("expired");
    });

    it("gets expiry from item with sections", () => {
      const item = {
        category: "api_credential" as const,
        sections: [
          {
            id: "main",
            title: null,
            fields: [
              { id: "expires", label: "expires", kind: "date" as const, value: "2026-05-27" },
            ],
          },
        ],
      };
      const res = getItemExpiry(item, NOW);
      expect(res?.status).toBe("expiring_soon");
      expect(res?.relativeHint).toBe("Expires in 12 days");
    });

    it("ignores non-card and non-api-credential categories", () => {
      const login = {
        category: "login" as const,
        expiry: "04/2026",
      };
      expect(getItemExpiry(login, NOW)).toBeNull();
    });
  });

  describe("translations with t function", () => {
    const tEn = (k: string, vars?: any) => translate("en", k, vars);
    const tMy = (k: string, vars?: any) => translate("my", k, vars);
    const tTh = (k: string, vars?: any) => translate("th", k, vars);

    it("formats relative hint in English", () => {
      const res = checkExpiry("03/2026", "month_year", NOW, tEn as any);
      expect(res?.relativeHint).toBe("Expired 2 months ago");
    });

    it("formats relative hint in Burmese", () => {
      const res = checkExpiry("03/2026", "month_year", NOW, tMy as any);
      expect(res?.relativeHint).toContain("လက သက်တမ်းကုန်သည်");
    });

    it("formats relative hint in Thai", () => {
      const res = checkExpiry("03/2026", "month_year", NOW, tTh as any);
      expect(res?.relativeHint).toContain("หมดอายุไปแล้ว 2 เดือน");
    });
  });
});
