    use super::*;
    use chrono::{Duration, Local};

    #[test]
    fn churn_buckets_distribute_by_local_date() {
        // 构造 3 个 item：今日 1 + 3 天前 1 + 8 天前 1（应被滤）
        let today = Local::now();
        let today_iso = today.format("%Y-%m-%dT%H:%M:%S%:z").to_string();
        let three_ago_iso = (today - Duration::days(3))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();
        let eight_ago_iso = (today - Duration::days(8))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();

        let mut cat = CategoryData {
            label: "test".to_string(),
            items: vec![],
        };
        for (title, ts) in [
            ("a", today_iso.clone()),
            ("b", three_ago_iso.clone()),
            ("c", eight_ago_iso.clone()),
        ] {
            cat.items.push(MemoryItem {
                title: title.to_string(),
                description: String::new(),
                detail_path: String::new(),
                created_at: ts.clone(),
                updated_at: ts,
            });
        }

        // 内联模拟 memory_category_churn_7d 对一个 cat 的处理逻辑（避开实际
        // memory_list 读盘）—— 确认日期换算 + bucket idx 正确。
        let today_date = today.date_naive();
        let mut buckets = [0u32; 7];
        for item in &cat.items {
            let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&item.updated_at)
            else {
                continue;
            };
            let local_date = dt.with_timezone(&Local).date_naive();
            let delta = (today_date - local_date).num_days();
            if (0..7).contains(&delta) {
                let idx = (6 - delta) as usize;
                buckets[idx] += 1;
            }
        }
        assert_eq!(buckets[6], 1, "today should land at idx 6");
        assert_eq!(buckets[3], 1, "3 days ago should land at idx 3");
        assert_eq!(buckets.iter().sum::<u32>(), 2, "8-days-ago item filtered out");
    }
