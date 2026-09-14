package com.tyu.app.feature.monitor

import android.content.Context
import android.view.SurfaceView

/** SurfaceView that letterboxes itself to the decoded video's aspect ratio instead of stretching. */
class AspectSurfaceView(context: Context) : SurfaceView(context) {
    private var videoWidth = 0
    private var videoHeight = 0

    fun setVideoSize(width: Int, height: Int) {
        if (width <= 0 || height <= 0 || (width == videoWidth && height == videoHeight)) return
        videoWidth = width
        videoHeight = height
        post { requestLayout() }
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val maxW = MeasureSpec.getSize(widthMeasureSpec)
        val maxH = MeasureSpec.getSize(heightMeasureSpec)
        if (videoWidth == 0 || videoHeight == 0 || maxW == 0 || maxH == 0) {
            super.onMeasure(widthMeasureSpec, heightMeasureSpec)
            return
        }
        val scale = minOf(maxW.toFloat() / videoWidth, maxH.toFloat() / videoHeight)
        setMeasuredDimension((videoWidth * scale).toInt(), (videoHeight * scale).toInt())
    }
}
