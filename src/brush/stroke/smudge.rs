//! Smudge bucket management + color sampling/mixing.
//!
//! 对应 mypaint-brush.c:906-1035：
//! - fetch_smudge_bucket (L906-918)
//! - update_smudge_color (L920-997)
//! - apply_smudge (L999-1035)

use super::{ACTUAL_RADIUS_MAX, ACTUAL_RADIUS_MIN};
use crate::brush::{Brush, SmudgeBucket};
use crate::smudge::mix_colors;
use crate::surface::Surface;
use crate::util::helpers::WGM_EPSILON;
use crate::BrushSetting;

impl Brush {
    /// 根据 SmudgeBucket setting 计算当前 step 应该读写的桶索引。
    /// 返回 `None` 表示无桶配置（caller 应回退到 inline_bucket）。
    /// 不变更状态。
    fn smudge_bucket_index(&self) -> Option<usize> {
        let buckets = self.smudge_buckets.as_ref()?;
        if buckets.is_empty() {
            return None;
        }
        let idx = self.settings_value[BrushSetting::SmudgeBucket as usize]
            .round()
            .clamp(0.0, buckets.len() as f32 - 1.0) as usize;
        Some(idx)
    }

    /// 取当前 stroke step 应该读写的 smudge bucket（可变）。
    /// 无桶配置时回退到 inline_bucket（等价于 C 的 STATE(SMUDGE_RA) 默认行为）。
    /// 对应 mypaint-brush.c:906-918，附带 min/max 跟踪。
    fn fetch_smudge_bucket_mut(&mut self) -> &mut SmudgeBucket {
        let Some(idx) = self.smudge_bucket_index() else {
            return &mut self.inline_bucket;
        };
        // min/max 跟踪，对应 mypaint-brush.c:911-916
        let bi = idx as i32;
        if self.min_bucket_used == -1 || self.min_bucket_used > bi {
            self.min_bucket_used = bi;
        }
        if self.max_bucket_used < bi {
            self.max_bucket_used = bi;
        }
        &mut self.smudge_buckets.as_mut().unwrap()[idx]
    }

    fn fetch_smudge_bucket_ref(&self) -> &SmudgeBucket {
        let Some(idx) = self.smudge_bucket_index() else {
            return &self.inline_bucket;
        };
        &self.smudge_buckets.as_ref().unwrap()[idx]
    }

    /// 对应 mypaint-brush.c:920-997 `update_smudge_color`。
    /// 返回 true 时表示 sample alpha 不达 op_lim 阈值，caller 应早返 false。
    ///
    /// `surface` 是外部 borrow 不依赖 `self`，与 `self.fetch_smudge_bucket_mut()`
    /// 的 `&mut self` 借用不冲突——所以这是 method 而不是 free fn。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_smudge_color(
        &mut self,
        surface: &mut dyn Surface,
        smudge_length: f32,
        smudge_length_log: f32,
        smudge_radius_log: f32,
        smudge_op_lim: f32,
        px: i32,
        py: i32,
        radius: f32,
        legacy_smudge: bool,
        paint_factor: f32,
    ) -> bool {
        let bucket = self.fetch_smudge_bucket_mut();
        // 对应 mypaint-brush.c:927-995。
        // update_factor 可能在 recentness==0 时被改为 0（首次初始化：直接用采样色）。
        let mut update_factor = 0.01f32.max(smudge_length);

        let recentness = bucket.recentness * update_factor;
        bucket.recentness = recentness;

        let margin = 0.0000000000000001;
        if recentness < 1.0f32.min((0.5 * update_factor).powf(smudge_length_log) + margin) {
            if recentness == 0.0 {
                // First initialization — sampled color used directly, no blend.
                // 对应 mypaint-brush.c:942-945
                update_factor = 0.0;
            }
            bucket.recentness = 1.0;

            let smudge_radius =
                (radius * smudge_radius_log.exp()).clamp(ACTUAL_RADIUS_MIN, ACTUAL_RADIUS_MAX);

            let (r, g, b, a) = surface.get_color(
                px as f32,
                py as f32,
                smudge_radius,
                if legacy_smudge { -1.0 } else { paint_factor },
            );

            if (smudge_op_lim > 0.0 && a < smudge_op_lim)
                || (smudge_op_lim < 0.0 && a > -smudge_op_lim)
            {
                return true;
            }
            bucket.prev = [r, g, b, a];
        }

        if legacy_smudge {
            let fac_old = update_factor;
            let fac_new = (1.0 - update_factor) * bucket.prev[3];
            bucket.smudge[0] = fac_old * bucket.smudge[0] + fac_new * bucket.prev[0];
            bucket.smudge[1] = fac_old * bucket.smudge[1] + fac_new * bucket.prev[1];
            bucket.smudge[2] = fac_old * bucket.smudge[2] + fac_new * bucket.prev[2];
            bucket.smudge[3] = (fac_old * bucket.smudge[3] + fac_new).clamp(0.0, 1.0);
        } else if bucket.prev[3] > WGM_EPSILON * 10.0 {
            let mixed = mix_colors(&bucket.smudge, &bucket.prev, update_factor, paint_factor);
            bucket.smudge = mixed;
        } else {
            bucket.smudge[3] = (bucket.smudge[3] + bucket.prev[3]) / 2.0;
        }
        false
    }

    /// 对应 mypaint-brush.c:999-1035 `apply_smudge`。
    #[inline]
    pub(super) fn apply_smudge(
        &self,
        smudge_value: f32,
        legacy_smudge: bool,
        paint_factor: f32,
        color_r: &mut f32,
        color_g: &mut f32,
        color_b: &mut f32,
    ) -> f32 {
        let bucket = self.fetch_smudge_bucket_ref();
        let smudge_factor = 1.0f32.min(smudge_value);
        let eraser_target_alpha =
            (1.0 - smudge_factor + smudge_factor * bucket.smudge[3]).clamp(0.0, 1.0);

        if eraser_target_alpha > 0.0 {
            if legacy_smudge {
                let col_factor = 1.0 - smudge_factor;
                *color_r = (smudge_factor * bucket.smudge[0] + col_factor * *color_r)
                    / eraser_target_alpha;
                *color_g = (smudge_factor * bucket.smudge[1] + col_factor * *color_g)
                    / eraser_target_alpha;
                *color_b = (smudge_factor * bucket.smudge[2] + col_factor * *color_b)
                    / eraser_target_alpha;
            } else {
                let brush_color = [*color_r, *color_g, *color_b, 1.0];
                let mixed = mix_colors(&bucket.smudge, &brush_color, smudge_factor, paint_factor);
                *color_r = mixed[0];
                *color_g = mixed[1];
                *color_b = mixed[2];
            }
        } else {
            *color_r = 1.0;
            *color_g = 0.0;
            *color_b = 0.0;
        }
        eraser_target_alpha
    }
}
