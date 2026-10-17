package dev.orgtree.hubchat

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Shader
import android.graphics.Typeface
import android.graphics.drawable.Icon
import android.util.Base64

/** Circular contact bitmap; no activity, network, disk cache or generic app icon. */
internal object LauncherAvatar {
  fun icon(context: Context, contact: LauncherContact): Icon {
    val night = context.getSharedPreferences("launcher-avatar", Context.MODE_PRIVATE).getBoolean("dark", true)
    val bitmap = Bitmap.createBitmap(128, 128, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap)
    val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    val profile = decode(contact.png)
    if (profile != null) {
      // The transparent photo sits over the same avatar tint and app surface
      // as the contact list, rather than the launcher's chosen panel colour.
      paint.color = LauncherAvatarRecipe.profileBackground(contact.kind, night)
      canvas.drawCircle(64f, 64f, 64f, paint)
      val shader = BitmapShader(profile, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP)
      val matrix = Matrix()
      matrix.setScale(128f / profile.width, 128f / profile.height)
      shader.setLocalMatrix(matrix)
      paint.shader = shader
      canvas.drawCircle(64f, 64f, 64f, paint)
      paint.shader = null
      profile.recycle()
    } else {
      val colors = LauncherAvatarRecipe.initialsColors(contact.kind, night)
      paint.color = colors.background
      canvas.drawCircle(64f, 64f, 64f, paint)
      paint.color = colors.foreground
      paint.typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
      paint.textSize = 128f * 16f / 44f
      paint.textAlign = Paint.Align.CENTER
      val text = LauncherAvatarRecipe.initials(contact.name)
      if (paint.measureText(text) > 116f) paint.textSize *= 116f / paint.measureText(text)
      canvas.drawText(text, 64f, 64f - (paint.ascent() + paint.descent()) / 2f, paint)
    }
    // A circular transparent bitmap is supported on API25, unlike adaptive
    // bitmaps (API26+), and avoids launcher masks cutting off the initials.
    return Icon.createWithBitmap(bitmap)
  }

  private fun decode(data: String?): Bitmap? = try {
    if (data == null || data.length > 350000) null else {
      val bytes = Base64.decode(data, Base64.NO_WRAP)
      val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
      BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
      if (bounds.outWidth !in 1..512 || bounds.outHeight != bounds.outWidth || bounds.outMimeType != "image/png") null
      else BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    }
  } catch (_: Exception) { null } // corrupted/unavailable bytes get real initials
}
