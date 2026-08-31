import SwiftUI

/// Simple placeholder for message image attachments. Once the services
/// return real images (URL or Data), you can extend this to render them.
struct ImageAttachmentView: View {
    let attachment: ImageAttachment

    var body: some View {
        switch attachment.source {
        case .url(let url):
            // For now, just show the URL as text; you can later replace
            // this with async image loading if desired.
            VStack(alignment: .leading, spacing: 4) {
                Text("Image URL:")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text(url.absoluteString)
                    .font(.footnote)
                    .foregroundColor(.blue)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
            }
            .padding(8)
            .background(Color.gray.opacity(0.1))
            .cornerRadius(8)

        case .data:
            // When you wire this up, you can render the Data as an Image.
            Rectangle()
                .fill(Color.gray.opacity(0.2))
                .frame(height: 200)
                .overlay(
                    Text("Image")
                        .foregroundColor(.secondary)
                )
                .cornerRadius(12)
        }
    }
}

