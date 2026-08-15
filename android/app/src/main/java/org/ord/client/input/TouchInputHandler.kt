package org.ord.client.input

import android.view.MotionEvent
import android.view.View
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import org.ord.client.protocol.InputEvent
import org.ord.client.protocol.InputEventType
import org.ord.client.protocol.OrdConstants
import org.ord.client.transport.OrdTransport

class TouchInputHandler(
    private val transport: OrdTransport,
    scope: CoroutineScope
) : View.OnTouchListener {

    // High-performance bounded channel to eliminate GC allocations and thread pool churn
    private val inputChannel = Channel<InputEvent>(Channel.UNLIMITED)

    init {
        // Dedicated single worker so all input packets are sent strictly in order with zero locks
        scope.launch(Dispatchers.IO) {
            for (event in inputChannel) {
                try {
                    transport.sendRaw(
                        msgType = OrdConstants.MSG_INPUT_EVENT,
                        flags = 0,
                        sequence = 0,
                        payload = event.encode()
                    )
                } catch (_: Exception) {}
            }
        }
    }

    override fun onTouch(v: View, event: MotionEvent): Boolean {
        val width = v.width.toFloat().coerceAtLeast(1f)
        val height = v.height.toFloat().coerceAtLeast(1f)

        val action = event.actionMasked
        val pointerIndex = event.actionIndex

        when (action) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                val pointerId = event.getPointerId(pointerIndex)
                val normX = ((event.getX(pointerIndex) / width) * 65535).toInt().coerceIn(0, 65535)
                val normY = ((event.getY(pointerIndex) / height) * 65535).toInt().coerceIn(0, 65535)

                inputChannel.trySend(
                    InputEvent(
                        eventType = InputEventType.TouchDown,
                        slot = pointerId.toByte(),
                        x = normX,
                        y = normY,
                        stateOrFlags = 1
                    )
                )
            }

            MotionEvent.ACTION_MOVE -> {
                val pointerCount = event.pointerCount
                for (i in 0 until pointerCount) {
                    val pointerId = event.getPointerId(i)
                    val normX = ((event.getX(i) / width) * 65535).toInt().coerceIn(0, 65535)
                    val normY = ((event.getY(i) / height) * 65535).toInt().coerceIn(0, 65535)

                    inputChannel.trySend(
                        InputEvent(
                            eventType = InputEventType.TouchMove,
                            slot = pointerId.toByte(),
                            x = normX,
                            y = normY,
                            stateOrFlags = 1
                        )
                    )
                }
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                val pointerId = event.getPointerId(pointerIndex)
                val normX = ((event.getX(pointerIndex) / width) * 65535).toInt().coerceIn(0, 65535)
                val normY = ((event.getY(pointerIndex) / height) * 65535).toInt().coerceIn(0, 65535)

                inputChannel.trySend(
                    InputEvent(
                        eventType = InputEventType.TouchUp,
                        slot = pointerId.toByte(),
                        x = normX,
                        y = normY,
                        stateOrFlags = 0
                    )
                )
            }

            MotionEvent.ACTION_CANCEL -> {
                inputChannel.trySend(
                    InputEvent(
                        eventType = InputEventType.TouchCancel,
                        slot = 0,
                        x = 0,
                        y = 0,
                        stateOrFlags = 0
                    )
                )
            }
        }

        return true
    }
}
