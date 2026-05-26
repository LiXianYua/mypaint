/* 用 C 调用 Rust 实现的 libmypaint。
 * 编译: gcc c_demo.c -L target/release -llibmypaint -lm -o c_demo
 * 运行: LD_LIBRARY_PATH=target/release ./c_demo
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Minimal subset of libmypaint headers — 我们只需要这些 symbol */
typedef struct MyPaintBrush MyPaintBrush;
typedef struct MyPaintSurface MyPaintSurface;
typedef struct MyPaintFixedTiledSurface MyPaintFixedTiledSurface;

extern void mypaint_init(void);
extern MyPaintBrush* mypaint_brush_new(void);
extern void mypaint_brush_unref(MyPaintBrush *self);
extern int mypaint_brush_from_string(MyPaintBrush *self, const char *string);

extern MyPaintFixedTiledSurface* mypaint_fixed_tiled_surface_new(int w, int h);
extern MyPaintSurface* mypaint_fixed_tiled_surface_interface(MyPaintFixedTiledSurface *self);

extern void mypaint_surface_begin_atomic(MyPaintSurface *self);
extern void mypaint_surface_end_atomic(MyPaintSurface *self, void *roi);
extern int  mypaint_surface_draw_dab(MyPaintSurface *self,
    float x, float y, float radius,
    float cr, float cg, float cb,
    float opaque, float hardness, float softness, float alpha_eraser,
    float aspect_ratio, float angle,
    float lock_alpha, float colorize, float posterize, float posterize_num,
    float paint);
extern void mypaint_surface_get_color(MyPaintSurface *self,
    float x, float y, float radius,
    float *cr, float *cg, float *cb, float *ca, float paint);
extern void mypaint_surface_save_png(MyPaintSurface *self, const char *path,
    int x, int y, int width, int height);
extern void mypaint_surface_unref(MyPaintSurface *self);

extern int mypaint_brush_stroke_to(MyPaintBrush *self, MyPaintSurface *surface,
    float x, float y, float pressure,
    float xtilt, float ytilt, double dtime,
    float viewzoom, float viewrotation, float barrel_rotation, int linear);

/* 简单读文件 */
static char* read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) { perror(path); return NULL; }
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *buf = malloc(sz + 1);
    fread(buf, 1, sz, f);
    buf[sz] = 0;
    fclose(f);
    return buf;
}

int main(void) {
    mypaint_init();
    printf("[1/5] mypaint_init OK\n");

    /* 加载笔刷 */
    char *brush_json = read_file("tests/brushes/charcoal.myb");
    if (!brush_json) return 1;
    MyPaintBrush *brush = mypaint_brush_new();
    if (!mypaint_brush_from_string(brush, brush_json)) {
        fprintf(stderr, "from_string failed\n");
        return 2;
    }
    free(brush_json);
    printf("[2/5] charcoal brush loaded\n");

    /* 创建画布 */
    MyPaintFixedTiledSurface *surf = mypaint_fixed_tiled_surface_new(300, 200);
    MyPaintSurface *iface = mypaint_fixed_tiled_surface_interface(surf);
    printf("[3/5] 300x200 surface created\n");

    /* 画一条线 */
    mypaint_surface_begin_atomic(iface);
    /* reset stroke */
    mypaint_brush_stroke_to(brush, iface, 50.0f, 100.0f, 0.0f,
        0.0f, 0.0f, 0.01, 1.0f, 0.0f, 0.0f, 0);
    /* 30 个有压力点 */
    for (int i = 1; i <= 30; i++) {
        float x = 50.0f + i * 7.0f;
        float y = 100.0f;
        mypaint_brush_stroke_to(brush, iface, x, y, 0.8f,
            0.0f, 0.0f, 0.01, 1.0f, 0.0f, 0.0f, 0);
    }
    mypaint_surface_end_atomic(iface, NULL);
    printf("[4/5] 30 dabs painted\n");

    /* 采样验证有像素 */
    float r, g, b, a;
    mypaint_surface_get_color(iface, 150.0f, 100.0f, 5.0f, &r, &g, &b, &a, 0.0f);
    printf("[5/5] sampled color: r=%.3f g=%.3f b=%.3f a=%.3f\n", r, g, b, a);

    /* 保存 PNG */
    mypaint_surface_save_png(iface, "c_demo.png", 0, 0, 300, 200);
    printf("[ok] saved c_demo.png\n");

    /* 清理 */
    mypaint_surface_unref(iface);
    mypaint_brush_unref(brush);
    return 0;
}
