
-- total
SELECT count(*) FROM media_item;

SELECT count(*), quick_file_type FROM media_item GROUP BY quick_file_type;
SELECT count(*), accurate_file_type FROM media_item GROUP BY accurate_file_type;

-- duplicates on long hash (unexpected)
SELECT count(*) cnt, long_hash FROM media_item GROUP BY long_hash HAVING cnt > 1 ORDER BY long_hash;
SELECT * FROM media_item WHERE long_hash = '00124897da003cfb0490232d10385e8e59ace947a6558245a0ab73343a2748d9';

-- duplicates on archive_item (expected)
SELECT count(*) cnt, media_item_id FROM archive_item GROUP BY media_item_id HAVING cnt > 1 ORDER BY media_item_id;

-- debug
SELECT * FROM archive_item where media_item_id in
 ( SELECT media_item_id FROM media_item where long_hash = '00124897da003cfb0490232d10385e8e59ace947a6558245a0ab73343a2748d9')
order by path;

-- duplicates on archive item path (unexpected)
SELECT count(*) cnt, path FROM archive_item GROUP BY path HAVING cnt > 1 ORDER BY path;

-- popular tags
SELECT count(*) cnt, tag_name FROM exif_tag GROUP BY tag_name ORDER BY cnt DESC;

-- percent of media with a particular tag
SELECT
    ROUND(
            (COUNT(CASE WHEN et.tag_name = 'Make' THEN 1 END) * 100.0) / COUNT(DISTINCT mi.media_item_id),
            2
    ) as percentage_with_make
FROM media_item mi
LEFT JOIN exif_tag et ON mi.media_item_id = et.media_item_id AND et.tag_name = 'Make';