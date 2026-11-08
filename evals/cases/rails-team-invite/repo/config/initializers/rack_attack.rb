class Rack::Attack
  throttle("invitations/user", limit: 20, period: 1.hour) do |req|
    req.env["warden"]&.user&.id if req.post? && req.path.match?(%r{\A/teams/\d+/invitations\z})
  end

  throttle("invitation-accept/ip", limit: 10, period: 1.minute) do |req|
    req.ip if req.post? && req.path.start_with?("/invitations/")
  end
end
