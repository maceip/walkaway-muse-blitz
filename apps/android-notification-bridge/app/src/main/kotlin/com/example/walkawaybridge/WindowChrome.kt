package com.example.walkawaybridge

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface

/**
 * Shaped approval windows drawn as bitmaps (approval design 2026-11-06:
 * "blooming corner" = credential/secret, "saddle dip" = spend). Same
 * chassis colour and body proportions per type; only the contour is the
 * signature. DAT flex has no z-layering, so the shaped window + text is
 * rendered here and real DAT buttons are docked beneath it.
 */
object WindowChrome {
  const val CHASSIS = 0xFF2B2118.toInt()
  const val HILITE = 0xFF3A2F22.toInt()
  const val INK = 0xFFEFEDE2.toInt()
  const val GOLD = 0xFFFFC400.toInt()
  const val GREEN = 0xFF008942.toInt()

  private fun paint(color: Int, textSize: Float, bold: Boolean = false) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    this.color = color
    this.textSize = textSize
    typeface = if (bold) Typeface.create(Typeface.DEFAULT, Typeface.BOLD) else Typeface.DEFAULT
  }

  /** Blooming-corner window: one top-right radius blooms open. */
  fun credential(
    title: String,
    account: String,
    note: String,
    w: Int = 1000,
    h: Int = 560
  ): Bitmap {
    val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    val c = Canvas(bmp)
    val d = w / 250f
    val r = 14f * d
    val bloom = 64f * d
    val path = Path().apply {
      moveTo(r, 0f)
      lineTo(w - bloom, 0f)
      arcTo(RectF(w - 2 * bloom, 0f, w.toFloat(), 2 * bloom), 270f, 90f, true)
      lineTo(w.toFloat(), h - r)
      arcTo(RectF(w - 2 * r, h - 2 * r, w.toFloat(), h.toFloat()), 0f, 90f, true)
      lineTo(r, h.toFloat())
      arcTo(RectF(0f, h - 2 * r, 2 * r, h.toFloat()), 90f, 90f, true)
      lineTo(0f, r)
      arcTo(RectF(0f, 0f, 2 * r, 2 * r), 180f, 90f, true)
      close()
    }
    c.drawPath(path, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = CHASSIS })
    val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = HILITE
      style = Paint.Style.STROKE
      strokeWidth = 3f * d
    }
    c.drawLine(28f * d, 2f * d, w - bloom + 4f * d, 2f * d, stroke)
    c.drawText(title, 36f * d, 110f * d, paint(INK, 42f * d, bold = true))
    c.drawText(account, 36f * d, 178f * d, paint(INK, 36f * d))
    c.drawText(note, 36f * d, 238f * d, paint(HILITE, 30f * d).apply { color = 0xB3EFEDE2.toInt() })
    return bmp
  }

  /** Saddle-dip window: top edge dips in one smooth arc. */
  fun spend(
    title: String,
    price: String,
    photoLabel: String = "photo",
    w: Int = 1000,
    h: Int = 560
  ): Bitmap {
    val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    val c = Canvas(bmp)
    val d = w / 250f
    val r = 14f * d
    val dip = 30f * d
    val path = Path().apply {
      moveTo(r, 0f)
      quadTo(w / 2f, dip, w - r, 0f)
      arcTo(RectF(w - 2 * r, 0f, w.toFloat(), 2 * r), 270f, 90f, true)
      lineTo(w.toFloat(), h - r)
      arcTo(RectF(w - 2 * r, h - 2 * r, w.toFloat(), h.toFloat()), 0f, 90f, true)
      lineTo(r, h.toFloat())
      arcTo(RectF(0f, h - 2 * r, 2 * r, h.toFloat()), 90f, 90f, true)
      lineTo(0f, r)
      arcTo(RectF(0f, 0f, 2 * r, 2 * r), 180f, 90f, true)
      close()
    }
    c.drawPath(path, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = CHASSIS })
    val chip = RectF(36f * d, 120f * d, 36f * d + 150f * d, 120f * d + 110f * d)
    c.drawRoundRect(chip, 10f * d, 10f * d, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = HILITE })
    c.drawText(photoLabel, chip.centerX(), chip.centerY() + 10f * d, paint(0xB3EFEDE2.toInt(), 28f * d))
    c.drawText(title, 210f * d, 160f * d, paint(INK, 42f * d, bold = true))
    c.drawText(price, 210f * d, 226f * d, paint(GOLD, 42f * d, bold = true))
    return bmp
  }
}
